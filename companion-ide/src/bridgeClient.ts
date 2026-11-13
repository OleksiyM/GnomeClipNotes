import { spawn, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';
import {
  BridgeRequest,
  BridgeMessage,
  BridgeResponse,
  BridgeError,
  PingResult,
  Folder,
  ItemSummary,
  ItemDetail,
  SaveItemResult
} from './types';

export interface BridgeClientOptions {
  bridgePath?: string;
  spawnFn?: (command: string, args: string[]) => ChildProcess;
  requestTimeoutMs?: number;
}

interface PendingRequest {
  resolve: (value: any) => void;
  reject: (reason: any) => void;
  timer: NodeJS.Timeout;
}

export class BridgeClient extends EventEmitter {
  private bridgePath: string;
  private spawnFn: (command: string, args: string[]) => ChildProcess;
  private requestTimeoutMs: number;
  private process: ChildProcess | null = null;
  private stdoutBuffer: string = '';
  private nextRequestId: number = 1;
  private pendingRequests: Map<string, PendingRequest> = new Map();
  private isDisposed: boolean = false;

  constructor(options: BridgeClientOptions = {}) {
    super();
    this.bridgePath = options.bridgePath || 'gnome-clip-notes';
    this.spawnFn = options.spawnFn || ((cmd, args) => spawn(cmd, args, { stdio: ['pipe', 'pipe', 'inherit'] }));
    this.requestTimeoutMs = options.requestTimeoutMs || 10000;
  }

  public setBridgePath(newPath: string): void {
    if (this.bridgePath !== newPath) {
      this.bridgePath = newPath;
      this.restart();
    }
  }

  public getBridgePath(): string {
    return this.bridgePath;
  }

  private ensureProcess(): ChildProcess {
    if (this.isDisposed) {
      throw new Error('BridgeClient is disposed');
    }
    if (this.process && !this.process.killed && this.process.exitCode === null) {
      return this.process;
    }

    const child = this.spawnFn(this.bridgePath, ['--bridge']);
    this.process = child;
    this.stdoutBuffer = '';

    child.stdout?.on('data', (chunk: Buffer | string) => {
      this.handleStdoutData(chunk.toString());
    });

    child.on('error', (err: Error) => {
      this.handleProcessFailure(new BridgeError({
        code: 'SPAWN_ERROR',
        message: `Failed to spawn bridge '${this.bridgePath}': ${err.message}`
      }));
    });

    child.on('exit', (code: number | null, signal: string | null) => {
      this.handleProcessFailure(new BridgeError({
        code: 'PROCESS_EXITED',
        message: `Bridge process exited with code ${code}, signal ${signal}`
      }));
      this.process = null;
    });

    return child;
  }

  private handleStdoutData(data: string): void {
    this.stdoutBuffer += data;
    const lines = this.stdoutBuffer.split('\n');
    // The last element is either empty (if ended in newline) or a partial line
    this.stdoutBuffer = lines.pop() || '';

    for (const rawLine of lines) {
      const line = rawLine.trim();
      if (!line) {
        continue;
      }
      this.handleLine(line);
    }
  }

  public handleLine(line: string): void {
    let msg: BridgeMessage;
    try {
      msg = JSON.parse(line);
    } catch (e: any) {
      // Ignore unparseable lines or malformed output
      return;
    }

    if (msg.type === 'response') {
      const pending = this.pendingRequests.get(msg.request_id);
      if (pending) {
        clearTimeout(pending.timer);
        this.pendingRequests.delete(msg.request_id);
        if (msg.ok) {
          pending.resolve(msg.result);
        } else {
          pending.reject(new BridgeError(msg.error));
        }
      }
    } else if (msg.type === 'event' && msg.event === 'changed') {
      this.emit('changed', msg.timestamp);
    }
  }

  private handleProcessFailure(error: Error): void {
    for (const [id, pending] of this.pendingRequests.entries()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pendingRequests.clear();
  }

  public sendRequest<T = any>(method: string, params: Record<string, any> = {}): Promise<T> {
    if (this.isDisposed) {
      return Promise.reject(new Error('BridgeClient is disposed'));
    }

    const requestId = String(this.nextRequestId++);
    const payload: BridgeRequest = {
      request_id: requestId,
      method,
      params
    };

    return new Promise<T>((resolve, reject) => {
      let proc: ChildProcess;
      try {
        proc = this.ensureProcess();
      } catch (e) {
        return reject(e);
      }

      const timer = setTimeout(() => {
        this.pendingRequests.delete(requestId);
        reject(new BridgeError({
          code: 'TIMEOUT',
          message: `Request '${method}' (id: ${requestId}) timed out after ${this.requestTimeoutMs}ms`
        }));
      }, this.requestTimeoutMs);

      this.pendingRequests.set(requestId, { resolve, reject, timer });

      const jsonStr = JSON.stringify(payload) + '\n';
      const written = proc.stdin?.write(jsonStr, 'utf-8');
      if (written === false) {
        proc.stdin?.once('drain', () => {});
      }
    });
  }

  public async ping(): Promise<PingResult> {
    return this.sendRequest<PingResult>('ping', {});
  }

  public async listFolders(includeHistory = false): Promise<Folder[]> {
    const res = await this.sendRequest<{ folders: Folder[] }>('list_folders', { include_history: includeHistory });
    return res.folders;
  }

  public async listItems(folderId: number): Promise<ItemSummary[]> {
    const res = await this.sendRequest<{ items: ItemSummary[] }>('list_items', { folder_id: folderId });
    return res.items;
  }

  public async readItem(id: number): Promise<ItemDetail> {
    return this.sendRequest<ItemDetail>('read_item', { id });
  }

  public async saveItem(
    id: number,
    content: string,
    expectedRevision: string,
    title?: string
  ): Promise<SaveItemResult> {
    const params: Record<string, any> = {
      id,
      content,
      expected_revision: expectedRevision
    };
    if (title !== undefined) {
      params.title = title;
    }
    return this.sendRequest<SaveItemResult>('save_item', params);
  }

  public restart(): void {
    if (this.process && !this.process.killed) {
      try {
        this.process.stdin?.end();
        this.process.kill('SIGTERM');
      } catch {
        // Ignore kill errors
      }
      this.process = null;
    }
    this.stdoutBuffer = '';
  }

  public dispose(): void {
    this.isDisposed = true;
    this.handleProcessFailure(new Error('BridgeClient was disposed'));
    if (this.process && !this.process.killed) {
      try {
        this.process.stdin?.end();
        this.process.kill('SIGTERM');
      } catch {
        // Ignore kill errors
      }
      this.process = null;
    }
    this.removeAllListeners();
  }
}

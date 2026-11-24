import { spawn, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  BridgeRequest,
  BridgeMessage,
  BridgeResponse,
  BridgeError,
  PingResult,
  Folder,
  ItemSummary,
  ItemDetail,
  ItemDetails,
  SaveItemResult
} from './types';

export const DEFAULT_BRIDGE_BINARY = 'gnome-clip-notes';

export interface BridgePathResolveDeps {
  homedir?: string;
  envPath?: string;
  isExecutableFile?: (filePath: string) => boolean;
}

/**
 * Checks whether `filePath` is an existing regular file with execute permission.
 */
export function defaultIsExecutableFile(filePath: string): boolean {
  try {
    const stat = fs.statSync(filePath);
    if (!stat.isFile()) {
      return false;
    }
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/**
 * Expands a leading `~` or `~/` segment to `homedir` without altering `~user/...`.
 */
export function expandLeadingTilde(inputPath: string, homedir: string): string {
  if (inputPath === '~') {
    return homedir;
  }
  if (inputPath.startsWith('~/') || inputPath.startsWith('~\\')) {
    return path.join(homedir, inputPath.slice(2));
  }
  return inputPath;
}

/**
 * Resolves the configured `gnomeClipNotes.bridgePath` value.
 * Trims whitespace, expands leading `~/`, and for the default binary falls back
 * to standard user install locations when not found on `PATH`.
 */
export function resolveBridgePath(rawPath?: string, deps: BridgePathResolveDeps = {}): string {
  const homedir = deps.homedir ?? os.homedir();
  const envPath = deps.envPath ?? process.env.PATH ?? '';
  const isExecutable = deps.isExecutableFile ?? defaultIsExecutableFile;
  const trimmed = (rawPath ?? '').trim();

  if (trimmed !== '' && trimmed !== DEFAULT_BRIDGE_BINARY) {
    return expandLeadingTilde(trimmed, homedir);
  }

  for (const dir of envPath.split(path.delimiter)) {
    if (!dir) {
      continue;
    }
    if (isExecutable(path.join(dir, DEFAULT_BRIDGE_BINARY))) {
      return DEFAULT_BRIDGE_BINARY;
    }
  }

  const standardCandidates = [
    path.join(homedir, 'Applications', 'GnomeClipNotes', DEFAULT_BRIDGE_BINARY),
    path.join(homedir, '.local', 'bin', DEFAULT_BRIDGE_BINARY)
  ];

  for (const candidate of standardCandidates) {
    if (isExecutable(candidate)) {
      return candidate;
    }
  }

  return DEFAULT_BRIDGE_BINARY;
}

export interface BridgeClientOptions {
  bridgePath?: string;
  spawnFn?: (command: string, args: string[]) => ChildProcess;
  requestTimeoutMs?: number;
  pathResolveDeps?: BridgePathResolveDeps;
}

interface PendingRequest {
  resolve: (value: any) => void;
  reject: (reason: any) => void;
  timer: NodeJS.Timeout;
}

export class BridgeClient extends EventEmitter {
  private configuredBridgePath: string;
  private spawnFn: (command: string, args: string[]) => ChildProcess;
  private requestTimeoutMs: number;
  private pathResolveDeps?: BridgePathResolveDeps;
  private process: ChildProcess | null = null;
  private stdoutBuffer: string = '';
  private nextRequestId: number = 1;
  private pendingRequests: Map<string, PendingRequest> = new Map();
  private isDisposed: boolean = false;

  constructor(options: BridgeClientOptions = {}) {
    super();
    this.configuredBridgePath = options.bridgePath ?? DEFAULT_BRIDGE_BINARY;
    this.spawnFn = options.spawnFn || ((cmd, args) => spawn(cmd, args, { stdio: ['pipe', 'pipe', 'inherit'] }));
    this.requestTimeoutMs = options.requestTimeoutMs || 10000;
    this.pathResolveDeps = options.pathResolveDeps;
  }

  public setBridgePath(newPath: string): void {
    if (this.configuredBridgePath !== newPath) {
      this.configuredBridgePath = newPath;
      this.restart();
    }
  }

  public getBridgePath(): string {
    return resolveBridgePath(this.configuredBridgePath, this.pathResolveDeps);
  }

  private ensureProcess(): ChildProcess {
    if (this.isDisposed) {
      throw new Error('BridgeClient is disposed');
    }
    if (this.process && !this.process.killed && this.process.exitCode === null) {
      return this.process;
    }

    const resolvedBridgePath = this.getBridgePath();
    const child = this.spawnFn(resolvedBridgePath, ['--bridge']);
    this.process = child;
    this.stdoutBuffer = '';

    child.stdout?.on('data', (chunk: Buffer | string) => {
      this.handleStdoutData(chunk.toString());
    });

    child.on('error', (err: Error) => {
      this.handleProcessFailure(new BridgeError({
        code: 'SPAWN_ERROR',
        message: `Failed to spawn bridge '${resolvedBridgePath}': ${err.message}`
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

    if (!msg || typeof msg !== 'object') {
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

  public async readItemDetails(id: number): Promise<ItemDetails> {
    return this.sendRequest<ItemDetails>('read_item_details', { id });
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

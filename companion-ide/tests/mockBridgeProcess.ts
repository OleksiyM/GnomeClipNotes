import { EventEmitter } from 'events';
import { Readable, Writable } from 'stream';
import { ChildProcess } from 'child_process';

export class MockBridgeProcess extends EventEmitter {
  public stdin: Writable;
  public stdout: Readable;
  public killed: boolean = false;
  public exitCode: number | null = null;

  public currentRevision: string = 'rev_init_1234567890abcdef12345678';
  public currentContent: string = '# Hello from GnomeClipNotes\nInitial content.';
  public currentTitle: string = 'Test Note';
  public currentFolderId: number = 1;

  public customHandler?: (req: any) => any;

  private buffer: string = '';

  constructor() {
    super();

    const self = this;
    this.stdout = new Readable({
      read() {}
    });

    this.stdin = new Writable({
      write(chunk, encoding, callback) {
        self.buffer += chunk.toString();
        const lines = self.buffer.split('\n');
        self.buffer = lines.pop() || '';
        for (const line of lines) {
          if (line.trim()) {
            self.handleIncomingRequest(line.trim());
          }
        }
        callback();
      }
    });
  }

  public sendLine(obj: any): void {
    this.stdout.push(JSON.stringify(obj) + '\n');
  }

  public sendEvent(eventObj: any): void {
    this.sendLine(eventObj);
  }

  private handleIncomingRequest(raw: string): void {
    let req: any;
    try {
      req = JSON.parse(raw);
    } catch {
      this.sendLine({
        type: 'response',
        request_id: null,
        ok: false,
        error: { code: 'INVALID_REQUEST', message: 'Malformed JSON' }
      });
      return;
    }

    if (this.customHandler) {
      const customRes = this.customHandler(req);
      if (customRes !== undefined) {
        if (customRes !== null) {
          this.sendLine(customRes);
        }
        return;
      }
    }

    const { request_id, method, params } = req;

    switch (method) {
      case 'ping':
        this.sendLine({
          type: 'response',
          request_id,
          ok: true,
          result: { version: '1.0.0', max_content: 1048576 }
        });
        break;

      case 'list_folders':
        this.sendLine({
          type: 'response',
          request_id,
          ok: true,
          result: {
            folders: [
              { id: 1, name: 'Notes', item_count: 1, position: 1 },
              { id: 2, name: 'Work', item_count: 0, position: 2 }
            ]
          }
        });
        break;

      case 'list_items':
        if (params?.folder_id === 1) {
          this.sendLine({
            type: 'response',
            request_id,
            ok: true,
            result: {
              items: [
                {
                  id: 42,
                  folder_id: 1,
                  title: this.currentTitle,
                  updated_at: 1000,
                  created_at: 900,
                  revision: this.currentRevision
                }
              ]
            }
          });
        } else {
          this.sendLine({
            type: 'response',
            request_id,
            ok: true,
            result: { items: [] }
          });
        }
        break;

      case 'read_item':
        if (params?.id === 42) {
          this.sendLine({
            type: 'response',
            request_id,
            ok: true,
            result: {
              id: 42,
              folder_id: this.currentFolderId,
              title: this.currentTitle,
              content: this.currentContent,
              updated_at: 1000,
              created_at: 900,
              revision: this.currentRevision
            }
          });
        } else {
          this.sendLine({
            type: 'response',
            request_id,
            ok: false,
            error: { code: 'NOT_FOUND', message: `Item #${params?.id} not found` }
          });
        }
        break;

      case 'save_item': {
        const { id, content, expected_revision } = params;
        if (id !== 42) {
          this.sendLine({
            type: 'response',
            request_id,
            ok: false,
            error: { code: 'NOT_FOUND', message: `Item #${id} not found` }
          });
          return;
        }

        if (content.length > 1048576) {
          this.sendLine({
            type: 'response',
            request_id,
            ok: false,
            error: {
              code: 'CONTENT_TOO_LARGE',
              message: 'Content exceeds 1 MiB',
              details: { max_bytes: 1048576, actual_bytes: content.length }
            }
          });
          return;
        }

        if (!content.trim() || content.includes('\0')) {
          this.sendLine({
            type: 'response',
            request_id,
            ok: false,
            error: {
              code: 'INVALID_CONTENT',
              message: 'Content cannot be empty or contain null bytes'
            }
          });
          return;
        }

        if (expected_revision !== this.currentRevision) {
          this.sendLine({
            type: 'response',
            request_id,
            ok: false,
            error: {
              code: 'CONFLICT',
              message: 'Revision mismatch',
              details: {
                current_revision: this.currentRevision,
                title: this.currentTitle,
                content: this.currentContent,
                updated_at: 1000
              }
            }
          });
          return;
        }

        // Success
        this.currentRevision = 'rev_new_' + Math.random().toString(36).substring(2, 10);
        this.currentContent = content;
        this.sendLine({
          type: 'response',
          request_id,
          ok: true,
          result: {
            id: 42,
            revision: this.currentRevision,
            updated_at: Math.floor(Date.now() / 1000)
          }
        });
        break;
      }

      default:
        this.sendLine({
          type: 'response',
          request_id,
          ok: false,
          error: { code: 'INVALID_REQUEST', message: `Unknown method '${method}'` }
        });
    }
  }

  public kill(signal?: string): boolean {
    this.killed = true;
    this.exitCode = 0;
    this.emit('exit', 0, signal || 'SIGTERM');
    return true;
  }
}

export function createMockSpawnFn(mockProc: MockBridgeProcess) {
  return (_cmd: string, _args: string[]): ChildProcess => {
    return mockProc as unknown as ChildProcess;
  };
}

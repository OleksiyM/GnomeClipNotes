import { EventEmitter as NodeEventEmitter } from 'events';

export class MockUri {
  constructor(
    public readonly scheme: string,
    public readonly path: string,
    public readonly fsPath: string
  ) {}

  public toString(): string {
    return `${this.scheme}://${this.path}`;
  }

  public static file(filePath: string): MockUri {
    return new MockUri('file', filePath, filePath);
  }

  public static parse(uriStr: string): MockUri {
    const parts = uriStr.split(':');
    const scheme = parts[0];
    const path = parts.slice(1).join(':').replace(/^\/+/, '');
    return new MockUri(scheme, path, '/' + path);
  }
}

export class MockEventEmitter<T> {
  private emitter = new NodeEventEmitter();

  public event = (listener: (e: T) => any): { dispose: () => void } => {
    this.emitter.on('data', listener);
    return {
      dispose: () => {
        this.emitter.off('data', listener);
      }
    };
  };

  public fire(data: T): void {
    this.emitter.emit('data', data);
  }
}

export class MockRelativePattern {
  constructor(public base: string, public pattern: string) {}
}

export class MockThemeIcon {
  constructor(public readonly id: string) {}
}

export class MockTreeItem {
  public description?: string;
  public contextValue?: string;
  public iconPath?: any;
  public tooltip?: string;
  public command?: any;

  constructor(
    public label: string,
    public collapsibleState: number = 0
  ) {}
}

export const MockTreeItemCollapsibleState = {
  None: 0,
  Collapsed: 1,
  Expanded: 2
};

export class MockFileSystemWatcher {
  private changeEmitter = new MockEventEmitter<MockUri>();
  private createEmitter = new MockEventEmitter<MockUri>();
  private deleteEmitter = new MockEventEmitter<MockUri>();

  public onDidChange = this.changeEmitter.event;
  public onDidCreate = this.createEmitter.event;
  public onDidDelete = this.deleteEmitter.event;

  public fireChange(uri: MockUri): void {
    this.changeEmitter.fire(uri);
  }

  public fireCreate(uri: MockUri): void {
    this.createEmitter.fire(uri);
  }

  public fireDelete(uri: MockUri): void {
    this.deleteEmitter.fire(uri);
  }

  public dispose(): void {}
}

export interface MockTextDocument {
  uri: MockUri;
  isDirty: boolean;
  getText: () => string;
}

export interface MockVscodeState {
  infoMessages: string[];
  warningMessages: string[];
  warningResponses: Map<string, string>;
  errorMessages: string[];
  executedCommands: Array<{ command: string; args: any[] }>;
  registeredCommands: Map<string, Function>;
  textDocuments: MockTextDocument[];
  activeFileSystemWatchers: MockFileSystemWatcher[];
  docSaveListeners: Array<(doc: MockTextDocument) => any>;
  contentProviders: Map<string, any>;
  configs: Map<string, any>;
  reset: () => void;
}

export const mockState: MockVscodeState = {
  infoMessages: [],
  warningMessages: [],
  warningResponses: new Map(),
  errorMessages: [],
  executedCommands: [],
  registeredCommands: new Map(),
  textDocuments: [],
  activeFileSystemWatchers: [],
  docSaveListeners: [],
  contentProviders: new Map(),
  configs: new Map<string, any>([
    ['gnomeClipNotes.bridgePath', 'gnome-clip-notes'],
    ['gnomeClipNotes.debounceMs', 50]
  ]),
  reset() {
    this.infoMessages = [];
    this.warningMessages = [];
    this.warningResponses.clear();
    this.errorMessages = [];
    this.executedCommands = [];
    this.registeredCommands.clear();
    this.textDocuments = [];
    this.activeFileSystemWatchers = [];
    this.docSaveListeners = [];
    this.contentProviders.clear();
    this.configs = new Map<string, any>([
      ['gnomeClipNotes.bridgePath', 'gnome-clip-notes'],
      ['gnomeClipNotes.debounceMs', 50]
    ]);
  }
};

export const mockVscode = {
  Uri: MockUri,
  EventEmitter: MockEventEmitter,
  RelativePattern: MockRelativePattern,
  ThemeIcon: MockThemeIcon,
  TreeItem: MockTreeItem,
  TreeItemCollapsibleState: MockTreeItemCollapsibleState,
  Disposable: {
    from: (...disposables: any[]) => ({
      dispose: () => {
        disposables.forEach((d) => d && typeof d.dispose === 'function' && d.dispose());
      }
    })
  },
  workspace: {
    textDocuments: mockState.textDocuments,
    openTextDocument: async (uriOrPath: any) => {
      const uri = typeof uriOrPath === 'string' ? MockUri.file(uriOrPath) : uriOrPath;
      const doc: MockTextDocument = {
        uri,
        isDirty: false,
        getText: () => ''
      };
      mockState.textDocuments.push(doc);
      return doc;
    },
    onDidSaveTextDocument: (callback: (doc: any) => any) => {
      mockState.docSaveListeners.push(callback);
      return {
        dispose: () => {
          const idx = mockState.docSaveListeners.indexOf(callback);
          if (idx !== -1) {
            mockState.docSaveListeners.splice(idx, 1);
          }
        }
      };
    },
    createFileSystemWatcher: (_pattern: any) => {
      const watcher = new MockFileSystemWatcher();
      mockState.activeFileSystemWatchers.push(watcher);
      return watcher;
    },
    registerTextDocumentContentProvider: (scheme: string, provider: any) => {
      mockState.contentProviders.set(scheme, provider);
      return {
        dispose: () => {
          mockState.contentProviders.delete(scheme);
        }
      };
    },
    getConfiguration: (_section?: string) => ({
      get: <T>(key: string, defaultValue?: T): T => {
        const fullKey = _section ? `${_section}.${key}` : key;
        if (mockState.configs.has(fullKey)) {
          return mockState.configs.get(fullKey);
        }
        return (defaultValue !== undefined ? defaultValue : undefined) as unknown as T;
      }
    }),
    onDidChangeConfiguration: (_cb: any) => ({ dispose: () => {} })
  },
  window: {
    showTextDocument: async (doc: any, _options?: any) => doc,
    showInformationMessage: async (msg: string, ..._items: string[]) => {
      mockState.infoMessages.push(msg);
      return undefined;
    },
    showWarningMessage: async (msg: string, ...items: string[]) => {
      mockState.warningMessages.push(msg);
      for (const [key, resp] of mockState.warningResponses.entries()) {
        if (msg.includes(key)) {
          return resp;
        }
      }
      return items[0];
    },
    showErrorMessage: async (msg: string, ..._items: string[]) => {
      mockState.errorMessages.push(msg);
      return undefined;
    },
    createTreeView: (_id: string, options: any) => ({
      ...options,
      dispose: () => {}
    }),
    registerTreeDataProvider: (_id: string, _provider: any) => ({
      dispose: () => {}
    }),
    showQuickPick: async (items: any[]) => items[0]
  },
  commands: {
    registerCommand: (command: string, callback: Function) => {
      mockState.registeredCommands.set(command, callback);
      return {
        dispose: () => {
          mockState.registeredCommands.delete(command);
        }
      };
    },
    executeCommand: async (command: string, ...args: any[]) => {
      mockState.executedCommands.push({ command, args });
      const handler = mockState.registeredCommands.get(command);
      if (handler) {
        return handler(...args);
      }
      return undefined;
    }
  }
};

// Install the mock into Module.prototype.require
const Module = require('module');
const originalRequire = Module.prototype.require;
Module.prototype.require = function (id: string) {
  if (id === 'vscode') {
    return mockVscode;
  }
  return originalRequire.apply(this, arguments);
};

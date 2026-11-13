import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as crypto from 'crypto';
import * as vscode from 'vscode';
import { BridgeClient } from './bridgeClient';
import { BridgeError, TrackedNote } from './types';

export interface ProjectionManagerOptions {
  sessionRootDir?: string;
  pid?: number;
  debounceMs?: number;
}

export class ConflictContentProvider implements vscode.TextDocumentContentProvider {
  public static readonly SCHEME = 'gnome-clip-notes-conflict';
  private docs: Map<string, string> = new Map();
  private onDidChangeEmitter = new vscode.EventEmitter<vscode.Uri>();
  public readonly onDidChange = this.onDidChangeEmitter.event;

  public setDocument(uri: vscode.Uri, content: string): void {
    this.docs.set(uri.toString(), content);
    this.onDidChangeEmitter.fire(uri);
  }

  public provideTextDocumentContent(uri: vscode.Uri): string {
    return this.docs.get(uri.toString()) ?? '';
  }

  public removeDocument(uri: vscode.Uri): void {
    this.docs.delete(uri.toString());
  }
}

export class ProjectionManager {
  private bridgeClient: BridgeClient;
  private sessionRootDir: string;
  private pid: number;
  private sessionDir: string;
  private debounceMs: number;
  private trackedNotes: Map<number, TrackedNote> = new Map();
  private filePathToId: Map<string, number> = new Map();
  private debounceTimers: Map<number, NodeJS.Timeout> = new Map();
  private selfWriteHashes: Map<string, { hash: string; timestamp: number }> = new Map();
  private fileWatcher: vscode.FileSystemWatcher | null = null;
  private disposables: vscode.Disposable[] = [];
  public readonly conflictProvider: ConflictContentProvider;

  constructor(bridgeClient: BridgeClient, options: ProjectionManagerOptions = {}) {
    this.bridgeClient = bridgeClient;
    this.pid = options.pid ?? process.pid;
    this.debounceMs = options.debounceMs ?? 250;
    this.conflictProvider = new ConflictContentProvider();

    if (options.sessionRootDir) {
      this.sessionRootDir = options.sessionRootDir;
    } else if (process.env.XDG_RUNTIME_DIR) {
      this.sessionRootDir = path.join(process.env.XDG_RUNTIME_DIR, 'gnome-clip-notes-ide');
    } else {
      const uid = process.getuid ? process.getuid() : 'default';
      this.sessionRootDir = path.join(os.tmpdir(), `gnome-clip-notes-ide-${uid}`);
    }

    this.sessionDir = path.join(this.sessionRootDir, String(this.pid));
  }

  public getSessionRootDir(): string {
    return this.sessionRootDir;
  }

  public getSessionDir(): string {
    return this.sessionDir;
  }

  public getTrackedNote(id: number): TrackedNote | undefined {
    return this.trackedNotes.get(id);
  }

  public getAllTrackedNotes(): TrackedNote[] {
    return Array.from(this.trackedNotes.values());
  }

  public setDebounceMs(ms: number): void {
    this.debounceMs = ms;
  }

  /**
   * Initialize directories, perform orphan cleanup, and register watchers and diff providers.
   */
  public activate(context?: vscode.ExtensionContext): void {
    this.ensureDirectories();
    this.cleanupOrphanSessions();

    // Register ConflictContentProvider
    const providerRegistration = vscode.workspace.registerTextDocumentContentProvider(
      ConflictContentProvider.SCHEME,
      this.conflictProvider
    );
    this.disposables.push(providerRegistration);
    if (context) {
      context.subscriptions.push(providerRegistration);
    }

    // Trigger A: Editor document save event
    const saveListener = vscode.workspace.onDidSaveTextDocument((doc) => {
      this.handleDocumentSaved(doc);
    });
    this.disposables.push(saveListener);
    if (context) {
      context.subscriptions.push(saveListener);
    }

    // Trigger B: FileSystemWatcher on session RAM directory (for AI agents / external fs writes)
    const pattern = new vscode.RelativePattern(this.sessionDir, '*.md');
    this.fileWatcher = vscode.workspace.createFileSystemWatcher(pattern);
    this.fileWatcher.onDidChange((uri) => this.handleFileChanged(uri.fsPath));
    this.fileWatcher.onDidCreate((uri) => this.handleFileChanged(uri.fsPath));
    this.disposables.push(this.fileWatcher);
    if (context) {
      context.subscriptions.push(this.fileWatcher);
    }

    // Listen to bridge 'changed' event for external sync
    this.bridgeClient.on('changed', () => {
      this.handleExternalChanged();
    });
  }

  /**
   * Ensures root and session directories exist with mode 0o700.
   */
  public ensureDirectories(): void {
    if (!fs.existsSync(this.sessionRootDir)) {
      fs.mkdirSync(this.sessionRootDir, { recursive: true, mode: 0o700 });
    }
    try {
      fs.chmodSync(this.sessionRootDir, 0o700);
    } catch {
      // Best effort chmod
    }

    if (!fs.existsSync(this.sessionDir)) {
      fs.mkdirSync(this.sessionDir, { recursive: true, mode: 0o700 });
    }
    try {
      fs.chmodSync(this.sessionDir, 0o700);
    } catch {
      // Best effort chmod
    }
  }

  /**
   * Clean up orphaned <pid> directories where the process is no longer running.
   */
  public cleanupOrphanSessions(): void {
    if (!fs.existsSync(this.sessionRootDir)) {
      return;
    }

    try {
      const entries = fs.readdirSync(this.sessionRootDir, { withFileTypes: true });
      for (const entry of entries) {
        if (!entry.isDirectory()) {
          continue;
        }
        if (!/^\d+$/.test(entry.name)) {
          continue;
        }

        const pidNum = parseInt(entry.name, 10);
        if (pidNum === this.pid) {
          continue;
        }

        if (!this.isProcessAlive(pidNum)) {
          const orphanPath = path.join(this.sessionRootDir, entry.name);
          try {
            fs.rmSync(orphanPath, { recursive: true, force: true });
          } catch (err) {
            console.warn(`Failed to clean orphan session directory ${orphanPath}:`, err);
          }
        }
      }
    } catch (err) {
      console.warn('Failed to read session root directory during orphan cleanup:', err);
    }
  }

  public isProcessAlive(pid: number): boolean {
    try {
      process.kill(pid, 0);
      return true;
    } catch (err: any) {
      if (err.code === 'EPERM') {
        // Process exists but belongs to another user
        return true;
      }
      return false;
    }
  }

  /**
   * Sanitize slug for safe filename generation without path traversal.
   */
  public sanitizeSlug(rawTitle: string): string {
    let slug = rawTitle
      .replace(/[/\\]/g, '_')
      .replace(/\.\./g, '_')
      .replace(/[^a-zA-Z0-9_\-\u0400-\u04FF]+/g, '_')
      .replace(/_+/g, '_')
      .replace(/^_+|_+$/g, '');

    if (!slug) {
      slug = 'note';
    }
    return slug.slice(0, 50);
  }

  public getFilePathForItem(id: number, title: string): string {
    const slug = this.sanitizeSlug(title);
    return path.join(this.sessionDir, `${id}_${slug}.md`);
  }

  public computeHash(content: string): string {
    return crypto.createHash('sha256').update(content, 'utf-8').digest('hex');
  }

  public recordSelfWrite(filePath: string, hash: string): void {
    this.selfWriteHashes.set(filePath, { hash, timestamp: Date.now() });
  }

  public isSelfWrite(filePath: string, currentHash: string): boolean {
    const record = this.selfWriteHashes.get(filePath);
    if (!record) {
      return false;
    }

    // Suppress if hash matches and timestamp is within 3000ms
    const isMatching = record.hash === currentHash && (Date.now() - record.timestamp < 3000);
    if (isMatching) {
      this.selfWriteHashes.delete(filePath);
      return true;
    }
    return false;
  }

  /**
   * Open note from bridge: writes RAM projection file and opens in editor.
   */
  public async openNote(id: number): Promise<vscode.TextDocument> {
    this.ensureDirectories();

    const item = await this.bridgeClient.readItem(id);
    const existing = this.trackedNotes.get(item.id);
    if (existing && fs.existsSync(existing.filePath)) {
      const openDoc = vscode.workspace.textDocuments.find(
        (d) => d.uri.fsPath === existing.filePath
      );
      const isDirty = openDoc ? openDoc.isDirty : false;
      const localContent = fs.readFileSync(existing.filePath, 'utf-8');
      const localHash = this.computeHash(localContent);

      if (isDirty || localHash !== existing.lastSyncedHash) {
        const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(existing.filePath));
        await vscode.window.showTextDocument(doc, { preview: false });

        if (item.revision !== existing.revision) {
          const choice = await vscode.window.showWarningMessage(
            `Note "${existing.title}" (#${existing.id}) has newer changes in GnomeClipNotes, and you have unsaved local edits.`,
            'View Diff',
            'Discard Local Changes',
            'Keep Local'
          );

          if (choice === 'View Diff') {
            await this.showConflictDiff(
              existing.id,
              item.content,
              item.revision,
              existing.filePath
            );
          } else if (choice === 'Discard Local Changes') {
            const newHash = this.computeHash(item.content);
            this.recordSelfWrite(existing.filePath, newHash);
            fs.writeFileSync(existing.filePath, item.content, { encoding: 'utf-8', mode: 0o600 });
            existing.revision = item.revision;
            existing.lastSyncedHash = newHash;
            existing.title = item.title;
          }
        }

        return doc;
      }
    }

    const filePath = this.getFilePathForItem(item.id, item.title);
    const contentHash = this.computeHash(item.content);

    // Record self-write to avoid echo-loop
    this.recordSelfWrite(filePath, contentHash);

    // Write file with 0o600 mode
    fs.writeFileSync(filePath, item.content, { encoding: 'utf-8', mode: 0o600 });
    try {
      fs.chmodSync(filePath, 0o600);
    } catch {
      // Best effort chmod
    }

    const tracked: TrackedNote = {
      id: item.id,
      folderId: item.folder_id,
      title: item.title,
      revision: item.revision,
      filePath,
      lastSyncedHash: contentHash,
      lastSyncedAt: Date.now()
    };
    this.trackedNotes.set(item.id, tracked);
    this.filePathToId.set(filePath, item.id);

    const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(filePath));
    await vscode.window.showTextDocument(doc, { preview: false });
    return doc;
  }

  /**
   * Handle onDidSaveTextDocument
   */
  private handleDocumentSaved(doc: vscode.TextDocument): void {
    this.handleFileChanged(doc.uri.fsPath);
  }

  /**
   * Handle FileSystemWatcher change/create or onDidSaveTextDocument
   */
  public handleFileChanged(filePath: string): void {
    const sessionPrefix = this.sessionDir.endsWith(path.sep)
      ? this.sessionDir
      : this.sessionDir + path.sep;
    if (!filePath.startsWith(sessionPrefix)) {
      return;
    }

    let itemId = this.filePathToId.get(filePath);
    if (itemId === undefined) {
      // Attempt to extract from filename: <id>_<slug>.md
      const base = path.basename(filePath);
      const match = /^(\d+)_.*\.md$/.exec(base);
      if (match) {
        itemId = parseInt(match[1], 10);
      }
    }

    if (itemId === undefined) {
      return;
    }

    // Debounce save per itemId
    if (this.debounceTimers.has(itemId)) {
      clearTimeout(this.debounceTimers.get(itemId)!);
    }

    const id = itemId;
    const timer = setTimeout(async () => {
      this.debounceTimers.delete(id);
      await this.saveTrackedNote(id);
    }, this.debounceMs);

    this.debounceTimers.set(id, timer);
  }

  /**
   * Save tracked note to bridge with echo-loop and conflict protections.
   */
  public async saveTrackedNote(itemId: number): Promise<boolean> {
    const tracked = this.trackedNotes.get(itemId);
    if (!tracked) {
      return false;
    }

    if (!fs.existsSync(tracked.filePath)) {
      return false;
    }

    let content: string;
    try {
      content = fs.readFileSync(tracked.filePath, 'utf-8');
    } catch (err) {
      console.error(`Failed to read file for note #${itemId}:`, err);
      return false;
    }

    const currentHash = this.computeHash(content);

    // Echo-loop prevention: check if this is an extension self-write
    if (this.isSelfWrite(tracked.filePath, currentHash)) {
      return false;
    }

    // If file content matches what we already synced, nothing to save
    if (currentHash === tracked.lastSyncedHash) {
      return false;
    }

    try {
      const result = await this.bridgeClient.saveItem(tracked.id, content, tracked.revision);
      tracked.revision = result.revision;
      tracked.lastSyncedHash = currentHash;
      tracked.lastSyncedAt = result.updated_at * 1000;
      return true;
    } catch (err: any) {
      return this.handleSaveError(tracked, content, err);
    }
  }

  /**
   * Handles errors from save_item according to spec.
   */
  private async handleSaveError(tracked: TrackedNote, content: string, err: any): Promise<boolean> {
    if (err instanceof BridgeError) {
      if (err.code === 'CONTENT_TOO_LARGE') {
        const actual = err.details?.actual_bytes ?? Buffer.byteLength(content, 'utf-8');
        vscode.window.showErrorMessage(
          `Cannot save Note #${tracked.id}: Content size (${actual} bytes) exceeds 1 MiB limit. The local RAM file has been kept intact.`
        );
        return false;
      }

      if (err.code === 'INVALID_CONTENT') {
        vscode.window.showErrorMessage(
          `Cannot save Note #${tracked.id}: Content cannot be empty or contain null characters. The local RAM file has been kept intact.`
        );
        return false;
      }

      if (err.code === 'CONFLICT') {
        const conflictDetails = err.details || {};
        vscode.window.showWarningMessage(
          `Conflict detected for Note #${tracked.id}: The note was modified in GnomeClipNotes. Opening diff view...`
        );
        await this.showConflictDiff(
          tracked.id,
          conflictDetails.content ?? '',
          conflictDetails.current_revision ?? 'remote',
          tracked.filePath
        );
        return false;
      }

      if (err.code === 'FORBIDDEN') {
        vscode.window.showErrorMessage(
          `Cannot save Note #${tracked.id}: Editing this item is forbidden (e.g. History item).`
        );
        return false;
      }
    }

    vscode.window.showErrorMessage(
      `Failed to save Note #${tracked.id}: ${err.message || String(err)}`
    );
    return false;
  }

  /**
   * Open VS Code Diff between remote DB version and local RAM file.
   */
  public async showConflictDiff(
    id: number,
    remoteContent: string,
    remoteRevision: string,
    localFilePath: string
  ): Promise<void> {
    const revShort = remoteRevision.slice(0, 8);
    const virtualUri = vscode.Uri.parse(
      `${ConflictContentProvider.SCHEME}:/Note-${id}-Remote-${revShort}.md`
    );
    this.conflictProvider.setDocument(virtualUri, remoteContent);

    const localUri = vscode.Uri.file(localFilePath);
    await vscode.commands.executeCommand(
      'vscode.diff',
      virtualUri,
      localUri,
      `GnomeClipNotes DB (v${revShort}) ↔ Local Note #${id}`
    );
  }

  /**
   * Handle external 'changed' event from bridge.
   */
  public async handleExternalChanged(): Promise<void> {
    for (const tracked of Array.from(this.trackedNotes.values())) {
      try {
        const item = await this.bridgeClient.readItem(tracked.id);
        if (item.revision === tracked.revision) {
          continue;
        }

        // Check if buffer is dirty in editor
        const openDoc = vscode.workspace.textDocuments.find(
          (doc) => doc.uri.fsPath === tracked.filePath
        );
        const isDirty = openDoc ? openDoc.isDirty : false;

        let localContent = '';
        if (fs.existsSync(tracked.filePath)) {
          localContent = fs.readFileSync(tracked.filePath, 'utf-8');
        }
        const localHash = this.computeHash(localContent);

        if (!isDirty && localHash === tracked.lastSyncedHash) {
          // Clean buffer and no unsaved local modifications: silently update RAM file
          const newHash = this.computeHash(item.content);
          this.recordSelfWrite(tracked.filePath, newHash);
          fs.writeFileSync(tracked.filePath, item.content, { encoding: 'utf-8', mode: 0o600 });
          tracked.revision = item.revision;
          tracked.lastSyncedHash = newHash;
          tracked.title = item.title;
        } else {
          // Buffer is dirty or local file was modified: do NOT overwrite silently!
          const choice = await vscode.window.showWarningMessage(
            `Note "${tracked.title}" (#${tracked.id}) was modified externally in GnomeClipNotes. You have unsaved local changes.`,
            'View Diff',
            'Discard Local Changes',
            'Keep Local'
          );

          if (choice === 'View Diff') {
            await this.showConflictDiff(
              tracked.id,
              item.content,
              item.revision,
              tracked.filePath
            );
          } else if (choice === 'Discard Local Changes') {
            const newHash = this.computeHash(item.content);
            this.recordSelfWrite(tracked.filePath, newHash);
            fs.writeFileSync(tracked.filePath, item.content, { encoding: 'utf-8', mode: 0o600 });
            tracked.revision = item.revision;
            tracked.lastSyncedHash = newHash;
            tracked.title = item.title;
          }
        }
      } catch (err) {
        console.warn(`Failed to sync external change for note #${tracked.id}:`, err);
      }
    }
  }

  /**
   * Clear session cache safely.
   */
  public clearCache(): void {
    for (const timer of this.debounceTimers.values()) {
      clearTimeout(timer);
    }
    this.debounceTimers.clear();

    if (fs.existsSync(this.sessionDir)) {
      try {
        const files = fs.readdirSync(this.sessionDir);
        for (const file of files) {
          fs.rmSync(path.join(this.sessionDir, file), { force: true });
        }
      } catch (err) {
        console.error('Failed to clear session cache files:', err);
      }
    }

    this.trackedNotes.clear();
    this.filePathToId.clear();
    this.selfWriteHashes.clear();
  }

  /**
   * Deactivate: clean up current session directory, cancel timers, and dispose resources.
   */
  public deactivate(): void {
    for (const timer of this.debounceTimers.values()) {
      clearTimeout(timer);
    }
    this.debounceTimers.clear();

    if (fs.existsSync(this.sessionDir)) {
      try {
        fs.rmSync(this.sessionDir, { recursive: true, force: true });
      } catch (err) {
        console.warn(`Failed to remove session directory on deactivate (${this.sessionDir}):`, err);
      }
    }

    this.trackedNotes.clear();
    this.filePathToId.clear();
    this.selfWriteHashes.clear();

    for (const d of this.disposables) {
      try {
        d.dispose();
      } catch {
        // ignore
      }
    }
    this.disposables = [];
  }
}

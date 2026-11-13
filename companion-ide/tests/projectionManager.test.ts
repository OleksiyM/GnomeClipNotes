import { test, describe, beforeEach, afterEach } from 'node:test';
import * as assert from 'node:assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { mockState, MockUri } from './mockVscode';
import { BridgeClient } from '../src/bridgeClient';
import { ProjectionManager } from '../src/projectionManager';
import { MockBridgeProcess, createMockSpawnFn } from './mockBridgeProcess';

describe('ProjectionManager & RAM Projection', () => {
  let tempBaseDir: string;
  let mockProc: MockBridgeProcess;
  let bridgeClient: BridgeClient;
  let manager: ProjectionManager;
  const testPid = 99999;

  beforeEach(() => {
    mockState.reset();
    tempBaseDir = fs.mkdtempSync(path.join(os.tmpdir(), 'gcn-test-'));
    mockProc = new MockBridgeProcess();
    bridgeClient = new BridgeClient({
      spawnFn: createMockSpawnFn(mockProc)
    });
    manager = new ProjectionManager(bridgeClient, {
      sessionRootDir: tempBaseDir,
      pid: testPid,
      debounceMs: 40
    });
    manager.activate();
  });

  afterEach(() => {
    manager.deactivate();
    bridgeClient.dispose();
    if (fs.existsSync(tempBaseDir)) {
      fs.rmSync(tempBaseDir, { recursive: true, force: true });
    }
  });

  test('directory creation and permissions (0700)', () => {
    manager.ensureDirectories();

    const rootDir = manager.getSessionRootDir();
    const sessionDir = manager.getSessionDir();

    assert.ok(fs.existsSync(rootDir));
    assert.ok(fs.existsSync(sessionDir));

    const rootStat = fs.statSync(rootDir);
    const sessionStat = fs.statSync(sessionDir);

    // Check mode permissions (0o700 = 040700 for directory)
    assert.strictEqual(rootStat.mode & 0o777, 0o700);
    assert.strictEqual(sessionStat.mode & 0o777, 0o700);
  });

  test('slug sanitization prevents path traversal and special characters', () => {
    assert.strictEqual(manager.sanitizeSlug('../../etc/passwd'), 'etc_passwd');
    assert.strictEqual(manager.sanitizeSlug('Hello / World \\ Test: Note!'), 'Hello_World_Test_Note');
    assert.strictEqual(manager.sanitizeSlug('   '), 'note');
    assert.strictEqual(manager.sanitizeSlug('Заметка №123 (План)'), 'Заметка_123_План');
  });

  test('file permissions (0600) on openNote', async () => {
    await manager.openNote(42);

    const tracked = manager.getTrackedNote(42);
    assert.ok(tracked);
    assert.ok(fs.existsSync(tracked.filePath));

    const fileStat = fs.statSync(tracked.filePath);
    // File mode 0o600
    assert.strictEqual(fileStat.mode & 0o777, 0o600);

    const content = fs.readFileSync(tracked.filePath, 'utf-8');
    assert.strictEqual(content, mockProc.currentContent);
  });

  test('orphan PID cleanup removes dead session directories', () => {
    const deadPidDir = path.join(tempBaseDir, '88888');
    const alivePidDir = path.join(tempBaseDir, String(process.pid));
    const nonPidDir = path.join(tempBaseDir, 'some_other_folder');

    fs.mkdirSync(deadPidDir, { mode: 0o700 });
    fs.writeFileSync(path.join(deadPidDir, 'orphan.md'), 'orphan note');

    fs.mkdirSync(alivePidDir, { mode: 0o700 });
    fs.mkdirSync(nonPidDir, { mode: 0o700 });

    manager.cleanupOrphanSessions();

    // Dead PID (88888) should be removed
    assert.strictEqual(fs.existsSync(deadPidDir), false);
    // Alive PID (current process) should remain
    assert.strictEqual(fs.existsSync(alivePidDir), true);
    // Non-PID dir should remain
    assert.strictEqual(fs.existsSync(nonPidDir), true);
  });

  test('echo-loop suppression ignores extension self-writes', async () => {
    await manager.openNote(42);
    const tracked = manager.getTrackedNote(42)!;

    // Simulate file watcher firing immediately after openNote (self-write)
    const savedCalledBefore = mockProc.currentRevision;
    manager.handleFileChanged(tracked.filePath);

    // Wait for debounce period
    await new Promise((r) => setTimeout(r, 80));

    // Revision should remain unchanged because self-write was suppressed
    assert.strictEqual(mockProc.currentRevision, savedCalledBefore);
  });

  test('FileSystemWatcher triggers debounced save on file modification', async () => {
    await manager.openNote(42);
    const tracked = manager.getTrackedNote(42)!;
    const initialRev = tracked.revision;

    // Simulate AI agent writing directly to RAM file via fs
    const newContent = '# AI Agent Update\nWritten via disk fs directly.';
    fs.writeFileSync(tracked.filePath, newContent, { encoding: 'utf-8', mode: 0o600 });

    // Trigger watcher multiple times rapidly (debounce test)
    manager.handleFileChanged(tracked.filePath);
    manager.handleFileChanged(tracked.filePath);
    manager.handleFileChanged(tracked.filePath);

    // Wait for debounce period to finish
    await new Promise((r) => setTimeout(r, 100));

    assert.notStrictEqual(tracked.revision, initialRev);
    assert.strictEqual(mockProc.currentContent, newContent);
    assert.strictEqual(tracked.lastSyncedHash, manager.computeHash(newContent));
  });

  test('CONFLICT opens diff view and preserves local RAM file intact', async () => {
    await manager.openNote(42);
    const tracked = manager.getTrackedNote(42)!;

    // Change remote revision on bridge behind the scenes
    mockProc.currentRevision = 'remote_conflicting_revision';
    mockProc.currentContent = '# Remote Server Content\nModified in GnomeClipNotes Library.';

    // Local user edits RAM file
    const localContent = '# Local Unsaved Edits\nDo not overwrite me!';
    fs.writeFileSync(tracked.filePath, localContent, { encoding: 'utf-8', mode: 0o600 });

    // Trigger save
    manager.handleFileChanged(tracked.filePath);
    await new Promise((r) => setTimeout(r, 100));

    // Verify warning was shown
    assert.ok(mockState.warningMessages.some((m) => m.includes('Conflict detected')));

    // Verify vscode.diff command was executed
    const diffCmd = mockState.executedCommands.find((c) => c.command === 'vscode.diff');
    assert.ok(diffCmd, 'vscode.diff command was executed');
    assert.strictEqual(diffCmd.args[1].fsPath, tracked.filePath);

    // Verify local RAM file was preserved
    const keptContent = fs.readFileSync(tracked.filePath, 'utf-8');
    assert.strictEqual(keptContent, localContent);
  });

  test('CONTENT_TOO_LARGE shows error and preserves local RAM file intact', async () => {
    await manager.openNote(42);
    const tracked = manager.getTrackedNote(42)!;

    // Write content > 1 MiB
    const largeContent = 'x'.repeat(1048577);
    fs.writeFileSync(tracked.filePath, largeContent, { encoding: 'utf-8', mode: 0o600 });

    manager.handleFileChanged(tracked.filePath);
    await new Promise((r) => setTimeout(r, 100));

    // Verify error was shown
    assert.ok(mockState.errorMessages.some((m) => m.includes('exceeds 1 MiB limit')));

    // Verify RAM file kept intact
    const keptContent = fs.readFileSync(tracked.filePath, 'utf-8');
    assert.strictEqual(keptContent, largeContent);
  });

  test('INVALID_CONTENT shows error and preserves local RAM file', async () => {
    await manager.openNote(42);
    const tracked = manager.getTrackedNote(42)!;

    // Write empty content
    fs.writeFileSync(tracked.filePath, '   \n  ', { encoding: 'utf-8', mode: 0o600 });

    manager.handleFileChanged(tracked.filePath);
    await new Promise((r) => setTimeout(r, 100));

    // Verify error was shown
    assert.ok(mockState.errorMessages.some((m) => m.includes('cannot be empty')));

    // File kept intact
    assert.ok(fs.existsSync(tracked.filePath));
  });

  test('external changed event updates clean file silently', async () => {
    await manager.openNote(42);
    const tracked = manager.getTrackedNote(42)!;

    // Remote changes
    mockProc.currentRevision = 'remote_rev_v2';
    mockProc.currentContent = '# Remote Clean Update\nSynced cleanly.';

    // Fire external changed
    await manager.handleExternalChanged();

    // Local file should be updated silently
    const updatedContent = fs.readFileSync(tracked.filePath, 'utf-8');
    assert.strictEqual(updatedContent, '# Remote Clean Update\nSynced cleanly.');
    assert.strictEqual(tracked.revision, 'remote_rev_v2');
  });

  test('external changed event warns when buffer has local modifications', async () => {
    await manager.openNote(42);
    const tracked = manager.getTrackedNote(42)!;

    // User made local changes
    fs.writeFileSync(tracked.filePath, '# Local Changes\nUnsaved', {
      encoding: 'utf-8',
      mode: 0o600
    });

    // Remote changes
    mockProc.currentRevision = 'remote_rev_v3';
    mockProc.currentContent = '# Remote Conflicting Update';

    // Mock warning response choice as Keep Local
    mockState.warningResponses.set('unsaved local changes', 'Keep Local');

    await manager.handleExternalChanged();

    assert.ok(mockState.warningMessages.some((m) => m.includes('unsaved local changes')));
    // File was NOT overwritten
    const content = fs.readFileSync(tracked.filePath, 'utf-8');
    assert.strictEqual(content, '# Local Changes\nUnsaved');
  });

  test('deactivate cleans up session directory and clears timers', async () => {
    await manager.openNote(42);
    assert.ok(fs.existsSync(manager.getSessionDir()));

    manager.deactivate();

    // Session directory should be deleted on deactivate
    assert.strictEqual(fs.existsSync(manager.getSessionDir()), false);
    assert.strictEqual(manager.getAllTrackedNotes().length, 0);
  });

  test('clearCache removes files in session directory without breaking tracking state', async () => {
    await manager.openNote(42);
    const tracked = manager.getTrackedNote(42)!;
    assert.ok(fs.existsSync(tracked.filePath));

    manager.clearCache();

    // File removed
    assert.strictEqual(fs.existsSync(tracked.filePath), false);
    assert.strictEqual(manager.getAllTrackedNotes().length, 0);
  });

  test('handleFileChanged ignores sibling directory prefix', async () => {
    await manager.openNote(42);
    const tracked = manager.getTrackedNote(42)!;
    const siblingDir = manager.getSessionDir() + '-sibling';
    fs.mkdirSync(siblingDir, { recursive: true, mode: 0o700 });
    const siblingFile = path.join(siblingDir, '42_Test_Note.md');
    fs.writeFileSync(siblingFile, 'should be ignored', 'utf-8');

    const initialRev = tracked.revision;
    manager.handleFileChanged(siblingFile);
    await new Promise((r) => setTimeout(r, 80));

    assert.strictEqual(tracked.revision, initialRev);
    fs.rmSync(siblingDir, { recursive: true, force: true });
  });

  test('openNote does not overwrite unsaved local edits', async () => {
    await manager.openNote(42);
    const tracked = manager.getTrackedNote(42)!;

    // Modify local file without saving to bridge
    const localDraft = '# Unsaved Local Draft\nKeep me safe!';
    fs.writeFileSync(tracked.filePath, localDraft, { encoding: 'utf-8', mode: 0o600 });

    // Re-open same note while remote also changed
    mockProc.currentRevision = 'remote_rev_newer';
    mockProc.currentContent = '# Remote Newer Content';
    mockState.warningResponses.set('unsaved local edits', 'Keep Local');

    await manager.openNote(42);

    const contentAfter = fs.readFileSync(tracked.filePath, 'utf-8');
    assert.strictEqual(contentAfter, localDraft);
    assert.ok(mockState.warningMessages.some((m) => m.includes('unsaved local edits')));
  });
});

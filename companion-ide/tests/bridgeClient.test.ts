import { test, describe, beforeEach, afterEach } from 'node:test';
import * as assert from 'node:assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import './mockVscode';
import {
  BridgeClient,
  DEFAULT_BRIDGE_BINARY,
  defaultIsExecutableFile,
  expandLeadingTilde,
  resolveBridgePath
} from '../src/bridgeClient';
import { BridgeError } from '../src/types';
import { MockBridgeProcess, createMockSpawnFn } from './mockBridgeProcess';

describe('BridgeClient Protocol & Communication', () => {
  let mockProc: MockBridgeProcess;
  let client: BridgeClient;

  beforeEach(() => {
    mockProc = new MockBridgeProcess();
    client = new BridgeClient({
      spawnFn: createMockSpawnFn(mockProc),
      requestTimeoutMs: 1000
    });
  });

  afterEach(() => {
    client.dispose();
  });

  test('ping returns version and max_content', async () => {
    const res = await client.ping();
    assert.strictEqual(res.version, '1.0.0');
    assert.strictEqual(res.max_content, 1048576);
  });

  test('listFolders returns list of folders', async () => {
    const folders = await client.listFolders();
    assert.strictEqual(folders.length, 2);
    assert.strictEqual(folders[0].name, 'Notes');
    assert.strictEqual(folders[0].id, 1);
  });

  test('listItems returns items for a folder', async () => {
    const items = await client.listItems(1);
    assert.strictEqual(items.length, 1);
    assert.strictEqual(items[0].id, 42);
    assert.strictEqual(items[0].title, 'Test Note');
  });

  test('readItem returns item detail', async () => {
    const item = await client.readItem(42);
    assert.strictEqual(item.id, 42);
    assert.strictEqual(item.title, 'Test Note');
    assert.ok(item.content.includes('Hello from GnomeClipNotes'));
    assert.strictEqual(item.pinned_at, 0);
    assert.strictEqual(item.revision, mockProc.currentRevision);
  });

  test('readItemDetails returns metadata and optional classification fields', async () => {
    mockProc.currentPinnedAt = 1200;
    mockProc.currentCategoryName = 'Work';
    mockProc.currentSubcategoryName = 'API';
    mockProc.currentComment = 'Important token';

    const details = await client.readItemDetails(42);
    assert.strictEqual(details.id, 42);
    assert.strictEqual(details.folder_name, 'Notes');
    assert.strictEqual(details.pinned_at, 1200);
    assert.strictEqual(details.category_name, 'Work');
    assert.strictEqual(details.subcategory_name, 'API');
    assert.strictEqual(details.comment, 'Important token');
    assert.ok(details.char_count > 0);
    assert.ok(details.word_count > 0);
  });

  test('saveItem updates revision on success', async () => {
    const initialRev = mockProc.currentRevision;
    const res = await client.saveItem(42, 'Updated note content', initialRev);
    assert.strictEqual(res.id, 42);
    assert.notStrictEqual(res.revision, initialRev);
    assert.strictEqual(mockProc.currentContent, 'Updated note content');
  });

  test('saveItem fails with CONFLICT on revision mismatch', async () => {
    await assert.rejects(
      async () => {
        await client.saveItem(42, 'New content', 'outdated_revision_token');
      },
      (err: any) => {
        assert.ok(err instanceof BridgeError);
        assert.strictEqual(err.code, 'CONFLICT');
        assert.strictEqual(err.details?.current_revision, mockProc.currentRevision);
        return true;
      }
    );
  });

  test('saveItem fails with CONTENT_TOO_LARGE when > 1MiB', async () => {
    const largeContent = 'a'.repeat(1048577);
    await assert.rejects(
      async () => {
        await client.saveItem(42, largeContent, mockProc.currentRevision);
      },
      (err: any) => {
        assert.ok(err instanceof BridgeError);
        assert.strictEqual(err.code, 'CONTENT_TOO_LARGE');
        assert.strictEqual(err.details?.max_bytes, 1048576);
        assert.strictEqual(err.details?.actual_bytes, 1048577);
        return true;
      }
    );
  });

  test('saveItem fails with INVALID_CONTENT on empty string', async () => {
    await assert.rejects(
      async () => {
        await client.saveItem(42, '   ', mockProc.currentRevision);
      },
      (err: any) => {
        assert.ok(err instanceof BridgeError);
        assert.strictEqual(err.code, 'INVALID_CONTENT');
        return true;
      }
    );
  });

  test('correlates multiple interleaved requests correctly by request_id', async () => {
    // Intercept custom responses to deliver out-of-order
    const delayedResponses: any[] = [];
    mockProc.customHandler = (req) => {
      if (req.method === 'ping') {
        delayedResponses.push({
          type: 'response',
          request_id: req.request_id,
          ok: true,
          result: { version: 'req_' + req.request_id, max_content: 1048576 }
        });
        return null; // Don't respond immediately
      }
      return undefined;
    };

    const p1 = client.ping();
    const p2 = client.ping();
    const p3 = client.ping();

    // Give a tick for requests to reach mock process
    await new Promise((r) => setTimeout(r, 10));

    assert.strictEqual(delayedResponses.length, 3);

    // Deliver responses in reverse order: 3, 1, 2
    mockProc.sendLine(delayedResponses[2]);
    mockProc.sendLine(delayedResponses[0]);
    mockProc.sendLine(delayedResponses[1]);

    const [r1, r2, r3] = await Promise.all([p1, p2, p3]);

    assert.strictEqual(r1.version, 'req_1');
    assert.strictEqual(r2.version, 'req_2');
    assert.strictEqual(r3.version, 'req_3');
  });

  test('handles async changed event without breaking requests', async () => {
    let changedFired = false;
    let eventTimestamp = 0;

    client.on('changed', (ts) => {
      changedFired = true;
      eventTimestamp = ts;
    });

    // Ensure bridge process is spawned before emitting async event
    const initRes = await client.ping();
    assert.strictEqual(initRes.version, '1.0.0');

    // Send async event
    mockProc.sendEvent({
      type: 'event',
      event: 'changed',
      timestamp: 1700000000
    });

    await new Promise((r) => setTimeout(r, 10));
    assert.strictEqual(changedFired, true);
    assert.strictEqual(eventTimestamp, 1700000000);

    // Verify regular requests still work after an event
    const res = await client.ping();
    assert.strictEqual(res.version, '1.0.0');
  });

  test('handles request timeout', async () => {
    mockProc.customHandler = () => null; // Never reply

    const shortClient = new BridgeClient({
      spawnFn: createMockSpawnFn(mockProc),
      requestTimeoutMs: 50
    });

    await assert.rejects(
      async () => {
        await shortClient.ping();
      },
      (err: any) => {
        assert.ok(err instanceof BridgeError);
        assert.strictEqual(err.code, 'TIMEOUT');
        return true;
      }
    );
    shortClient.dispose();
  });
});

describe('Bridge Binary Path Resolution', () => {
  const mockHome = '/home/mockuser';

  test('trims whitespace and expands leading ~/ on custom paths without fallback', () => {
    assert.strictEqual(expandLeadingTilde('~', mockHome), mockHome);
    assert.strictEqual(expandLeadingTilde('~/bin/gcn', mockHome), path.join(mockHome, 'bin/gcn'));
    assert.strictEqual(expandLeadingTilde('~other/bin/gcn', mockHome), '~other/bin/gcn');

    const resolved = resolveBridgePath('  ~/builds/gnome-clip-notes \n', {
      homedir: mockHome,
      envPath: '',
      isExecutableFile: () => false
    });
    assert.strictEqual(resolved, path.join(mockHome, 'builds/gnome-clip-notes'));

    const otherUser = resolveBridgePath(' ~other/gnome-clip-notes ', {
      homedir: mockHome,
      envPath: '/usr/bin',
      isExecutableFile: () => true
    });
    assert.strictEqual(otherUser, '~other/gnome-clip-notes');
  });

  test('default bridgePath prefers PATH, then ~/Applications, then ~/.local/bin, then default name', () => {
    const appPath = path.join(mockHome, 'Applications', 'GnomeClipNotes', DEFAULT_BRIDGE_BINARY);
    const localBinPath = path.join(mockHome, '.local', 'bin', DEFAULT_BRIDGE_BINARY);

    // 1. Found on PATH
    assert.strictEqual(
      resolveBridgePath('  gnome-clip-notes  ', {
        homedir: mockHome,
        envPath: `/usr/local/bin${path.delimiter}/usr/bin`,
        isExecutableFile: (p) => p === path.join('/usr/bin', DEFAULT_BRIDGE_BINARY) || p === appPath
      }),
      DEFAULT_BRIDGE_BINARY
    );

    // 2. Missing from PATH -> prefers ~/Applications/GnomeClipNotes/gnome-clip-notes over ~/.local/bin
    assert.strictEqual(
      resolveBridgePath(DEFAULT_BRIDGE_BINARY, {
        homedir: mockHome,
        envPath: '/usr/bin',
        isExecutableFile: (p) => p === appPath || p === localBinPath
      }),
      appPath
    );

    // 3. Only ~/.local/bin/gnome-clip-notes exists (also when rawPath is empty/whitespace)
    assert.strictEqual(
      resolveBridgePath('   ', {
        homedir: mockHome,
        envPath: '/usr/bin',
        isExecutableFile: (p) => p === localBinPath
      }),
      localBinPath
    );

    // 4. None exist -> returns default binary name
    assert.strictEqual(
      resolveBridgePath(undefined, {
        homedir: mockHome,
        envPath: '/usr/bin',
        isExecutableFile: () => false
      }),
      DEFAULT_BRIDGE_BINARY
    );
  });

  test('defaultIsExecutableFile requires regular file with execute permission', () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'gcn-ide-exec-'));
    try {
      const missingPath = path.join(tmpDir, 'missing');
      const dirCandidate = path.join(tmpDir, 'dir-candidate');
      const nonExecFile = path.join(tmpDir, 'non-exec');
      const execFile = path.join(tmpDir, 'exec-bin');

      fs.mkdirSync(dirCandidate, { mode: 0o755 });
      fs.writeFileSync(nonExecFile, '#!/bin/sh\n', { mode: 0o644 });
      fs.writeFileSync(execFile, '#!/bin/sh\n', { mode: 0o755 });

      assert.strictEqual(defaultIsExecutableFile(missingPath), false);
      assert.strictEqual(defaultIsExecutableFile(dirCandidate), false);
      assert.strictEqual(defaultIsExecutableFile(nonExecFile), false);
      assert.strictEqual(defaultIsExecutableFile(execFile), true);
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  test('BridgeClient spawns resolved path and updates after setBridgePath', async () => {
    const spawnedCommands: string[] = [];
    const expectedAppPath = path.join(mockHome, 'Applications', 'GnomeClipNotes', DEFAULT_BRIDGE_BINARY);

    const client = new BridgeClient({
      bridgePath: '  gnome-clip-notes ',
      pathResolveDeps: {
        homedir: mockHome,
        envPath: '',
        isExecutableFile: (p) => p === expectedAppPath
      },
      spawnFn: (cmd, args) => {
        spawnedCommands.push(cmd);
        return createMockSpawnFn(new MockBridgeProcess())(cmd, args);
      }
    });

    assert.strictEqual(client.getBridgePath(), expectedAppPath);
    await client.ping();
    assert.deepStrictEqual(spawnedCommands, [expectedAppPath]);

    client.setBridgePath('  ~/custom-bin  ');
    assert.strictEqual(client.getBridgePath(), path.join(mockHome, 'custom-bin'));
    await client.ping();
    assert.deepStrictEqual(spawnedCommands, [expectedAppPath, path.join(mockHome, 'custom-bin')]);
    client.dispose();
  });
});

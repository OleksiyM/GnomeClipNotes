import { test, describe, beforeEach, afterEach } from 'node:test';
import * as assert from 'node:assert';
import './mockVscode';
import { BridgeClient } from '../src/bridgeClient';
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
    assert.strictEqual(item.revision, mockProc.currentRevision);
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

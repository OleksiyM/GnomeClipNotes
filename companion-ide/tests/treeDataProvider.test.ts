import { test, describe, beforeEach, afterEach } from 'node:test';
import * as assert from 'node:assert';
import './mockVscode';
import { BridgeClient } from '../src/bridgeClient';
import { GnomeClipNotesTreeDataProvider, FolderNode, NoteNode } from '../src/treeDataProvider';
import { MockBridgeProcess, createMockSpawnFn } from './mockBridgeProcess';

describe('GnomeClipNotesTreeDataProvider', () => {
  let mockProc: MockBridgeProcess;
  let bridgeClient: BridgeClient;
  let treeProvider: GnomeClipNotesTreeDataProvider;

  beforeEach(() => {
    mockProc = new MockBridgeProcess();
    bridgeClient = new BridgeClient({
      spawnFn: createMockSpawnFn(mockProc)
    });
    treeProvider = new GnomeClipNotesTreeDataProvider(bridgeClient);
  });

  afterEach(() => {
    bridgeClient.dispose();
  });

  test('getChildren without element returns FolderNodes', async () => {
    const rootNodes = await treeProvider.getChildren();
    assert.strictEqual(rootNodes.length, 2);
    assert.ok(rootNodes[0] instanceof FolderNode);
    assert.strictEqual((rootNodes[0] as FolderNode).folder.name, 'Notes');
    assert.strictEqual((rootNodes[1] as FolderNode).folder.name, 'Work');
  });

  test('getChildren with FolderNode returns NoteNodes', async () => {
    const rootNodes = await treeProvider.getChildren();
    const folderNode = rootNodes[0] as FolderNode;

    const childNodes = await treeProvider.getChildren(folderNode);
    assert.strictEqual(childNodes.length, 1);
    assert.ok(childNodes[0] instanceof NoteNode);
    assert.strictEqual((childNodes[0] as NoteNode).item.id, 42);
    assert.strictEqual((childNodes[0] as NoteNode).item.title, 'Test Note');
  });

  test('getTreeItem for FolderNode formats description and icon', () => {
    const folderNode = new FolderNode({
      id: 1,
      name: 'Notes',
      item_count: 5,
      position: 1
    });

    const item = treeProvider.getTreeItem(folderNode);
    assert.strictEqual(item.label, 'Notes');
    assert.strictEqual(item.description, '(5)');
    assert.strictEqual(item.contextValue, 'folder');
  });

  test('getTreeItem for NoteNode configures openNote command and fallback title', () => {
    const noteWithTitle = new NoteNode({
      id: 42,
      folder_id: 1,
      title: 'Meeting Notes',
      updated_at: 1000,
      created_at: 900,
      revision: 'rev1'
    });

    const item1 = treeProvider.getTreeItem(noteWithTitle);
    assert.strictEqual(item1.label, 'Meeting Notes');
    assert.strictEqual(item1.command?.command, 'gnomeClipNotes.openNote');
    assert.deepStrictEqual(item1.command?.arguments, [42]);

    const noteWithoutTitle = new NoteNode({
      id: 99,
      folder_id: 1,
      title: '   ',
      updated_at: 1000,
      created_at: 900,
      revision: 'rev1'
    });

    const item2 = treeProvider.getTreeItem(noteWithoutTitle);
    assert.strictEqual(item2.label, 'Note #99');
  });

  test('bridge changed event triggers refresh', (t, done) => {
    treeProvider.onDidChangeTreeData(() => {
      done();
    });

    bridgeClient.emit('changed', Date.now());
  });
});

import { test, describe, beforeEach, afterEach } from 'node:test';
import * as assert from 'node:assert';
import { mockState, MockMarkdownString, MockThemeIcon } from './mockVscode';
import { BridgeClient } from '../src/bridgeClient';
import {
  GnomeClipNotesTreeDataProvider,
  FolderNode,
  NoteNode,
  buildItemDetailsTooltip,
  escapeMarkdown,
  formatTimestamp
} from '../src/treeDataProvider';
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
    assert.strictEqual((item2.iconPath as MockThemeIcon).id, 'file-text');
    assert.strictEqual(item2.tooltip, undefined);
  });

  test('getTreeItem uses pinned icon when pinned_at > 0 and file-text otherwise', () => {
    const pinnedNode = new NoteNode({
      id: 42,
      folder_id: 1,
      title: 'Pinned Note',
      updated_at: 1000,
      created_at: 900,
      pinned_at: 1700000100,
      revision: 'rev1'
    });
    const unpinnedNode = new NoteNode({
      id: 43,
      folder_id: 1,
      title: 'Regular Note',
      updated_at: 1000,
      created_at: 900,
      pinned_at: 0,
      revision: 'rev2'
    });

    const pinnedItem = treeProvider.getTreeItem(pinnedNode);
    const unpinnedItem = treeProvider.getTreeItem(unpinnedNode);

    assert.strictEqual((pinnedItem.iconPath as MockThemeIcon).id, 'pinned');
    assert.strictEqual((unpinnedItem.iconPath as MockThemeIcon).id, 'file-text');
    assert.strictEqual(pinnedItem.tooltip, undefined);
  });

  test('resolveTreeItem populates inert MarkdownString tooltip with classification, escaped comment, and metadata', async () => {
    mockState.reset();
    mockProc.currentPinnedAt = 1700000500;
    mockProc.currentCategoryName = 'Projects [v2]';
    mockProc.currentSubcategoryName = 'API';
    mockProc.currentComment = 'Check [link](command:evil) & $(zap) icon\nSecond line *bold*';
    mockProc.currentSource = 'Firefox';
    mockProc.currentOrigin = 'clipboard';

    const noteNode = new NoteNode({
      id: 42,
      folder_id: 1,
      title: 'Test Note',
      updated_at: 1000,
      created_at: 900,
      pinned_at: 1700000500,
      revision: mockProc.currentRevision
    });

    const treeItem = treeProvider.getTreeItem(noteNode);
    const resolved = await treeProvider.resolveTreeItem(treeItem, noteNode);

    assert.ok(resolved.tooltip instanceof MockMarkdownString);
    const md = resolved.tooltip as MockMarkdownString;
    assert.strictEqual(md.isTrusted, false);
    assert.strictEqual(md.supportThemeIcons, true);
    assert.ok(md.value.includes('$(folder) Notes · $(pinned) Pinned'));
    assert.ok(md.value.includes(`$(tag) Category: ${escapeMarkdown('Projects [v2]')} · API`));
    assert.ok(
      md.value.includes(
        '$(comment) Comment:  \nCheck \\[link\\]\\(command:evil\\) & \\$\\(zap\\) icon  \nSecond line \\*bold\\*'
      )
    );
    assert.ok(md.value.includes('Clipboard · Firefox'));
    assert.ok(md.value.includes('Created: '));
    assert.ok(md.value.includes('Edited: '));
    assert.ok(md.value.includes('Last Copied: '));
    assert.ok(md.value.includes('Pinned At: '));
    assert.ok(md.value.includes('characters · '));
    assert.ok(!md.value.includes('**Source:**'));
    assert.ok(!md.value.includes('Length:'));
    assert.strictEqual(mockState.errorMessages.length, 0);
  });

  test('resolveTreeItem falls back silently when read_item_details fails (1.6.0 bridge compatibility)', async () => {
    mockState.reset();
    mockProc.customHandler = (req) => {
      if (req.method === 'read_item_details') {
        return {
          type: 'response',
          request_id: req.request_id,
          ok: false,
          error: { code: 'INVALID_REQUEST', message: 'Unknown method: read_item_details' }
        };
      }
      return undefined;
    };

    const noteNode = new NoteNode({
      id: 42,
      folder_id: 1,
      title: 'Legacy Note',
      updated_at: 1000,
      created_at: 900,
      pinned_at: 1700000500,
      revision: 'rev1'
    });

    const treeItem = treeProvider.getTreeItem(noteNode);
    const resolved = await treeProvider.resolveTreeItem(treeItem, noteNode);

    assert.strictEqual(typeof resolved.tooltip, 'string');
    assert.ok((resolved.tooltip as string).includes('Legacy Note'));
    assert.ok((resolved.tooltip as string).includes('Pinned'));
    assert.strictEqual(mockState.errorMessages.length, 0);
  });

  test('formatTimestamp formats local 24-hour dates matching native Item Details', () => {
    assert.strictEqual(formatTimestamp(0), '');
    const sample = Math.floor(new Date(2026, 10, 22, 20, 5, 53).getTime() / 1000);
    assert.strictEqual(formatTimestamp(sample), 'Nov 22, 2026 · 20:05');
  });

  test('buildItemDetailsTooltip omits duplicate Edited/Last Copied timestamps and empty source', () => {
    const md = buildItemDetailsTooltip({
      id: 7,
      folder_id: 1,
      folder_name: 'Notes',
      title: 'Quick Memo',
      kind: 'text',
      origin: 'manual',
      source: '',
      source_id: '',
      created_at: 1000,
      updated_at: 1000,
      copied_at: 1000,
      pinned_at: 0,
      char_count: 1,
      word_count: 1,
      category_name: null,
      subcategory_name: null,
      comment: null
    });

    assert.ok(md.value.includes('\nNote\n\nCreated: '));
    assert.ok(!md.value.includes('Edited:'));
    assert.ok(!md.value.includes('Last Copied:'));
    assert.ok(!md.value.includes('Pinned At:'));
    assert.ok(!md.value.includes('Unknown'));
    assert.ok(md.value.includes('1 character · 1 word'));
  });

  test('bridge changed event triggers refresh', (t, done) => {
    treeProvider.onDidChangeTreeData(() => {
      done();
    });

    bridgeClient.emit('changed', Date.now());
  });
});

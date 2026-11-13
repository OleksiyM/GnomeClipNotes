import { test, describe, beforeEach, afterEach } from 'node:test';
import * as assert from 'node:assert';
import { mockState } from './mockVscode';
import * as vscode from 'vscode';
import { activate, deactivate } from '../src/extension';

describe('Extension Activation & Commands', () => {
  const dummyContext: any = {
    subscriptions: []
  };

  beforeEach(() => {
    mockState.reset();
  });

  afterEach(() => {
    deactivate();
  });

  test('activate registers commands and tree view', () => {
    activate(dummyContext);

    assert.ok(mockState.registeredCommands.has('gnomeClipNotes.refresh'));
    assert.ok(mockState.registeredCommands.has('gnomeClipNotes.openNote'));
    assert.ok(mockState.registeredCommands.has('gnomeClipNotes.clearCache'));

    assert.ok(dummyContext.subscriptions.length > 0);
  });

  test('gnomeClipNotes.clearCache command executes cleanly', async () => {
    activate(dummyContext);

    await vscode.commands.executeCommand('gnomeClipNotes.clearCache');

    assert.ok(
      mockState.infoMessages.some((msg) =>
        msg.includes('GnomeClipNotes cache cleared successfully')
      )
    );
  });

  test('deactivate cleans up resources without errors', () => {
    activate(dummyContext);
    assert.doesNotThrow(() => {
      deactivate();
    });
  });
});

import * as vscode from 'vscode';
import { BridgeClient } from './bridgeClient';
import { ProjectionManager } from './projectionManager';
import { GnomeClipNotesTreeDataProvider, NoteNode } from './treeDataProvider';

let bridgeClient: BridgeClient | null = null;
let projectionManager: ProjectionManager | null = null;
let treeDataProvider: GnomeClipNotesTreeDataProvider | null = null;

export function activate(context: vscode.ExtensionContext): void {
  const config = vscode.workspace.getConfiguration('gnomeClipNotes');
  const bridgePath = config.get<string>('bridgePath', 'gnome-clip-notes');
  const debounceMs = config.get<number>('debounceMs', 250);

  bridgeClient = new BridgeClient({ bridgePath });
  projectionManager = new ProjectionManager(bridgeClient, { debounceMs });
  projectionManager.activate(context);

  treeDataProvider = new GnomeClipNotesTreeDataProvider(bridgeClient);

  const treeView = vscode.window.createTreeView('gnomeClipNotesFolders', {
    treeDataProvider,
    showCollapseAll: true
  });
  context.subscriptions.push(treeView);

  // Commands
  const refreshCommand = vscode.commands.registerCommand(
    'gnomeClipNotes.refresh',
    () => {
      treeDataProvider?.refresh();
    }
  );
  context.subscriptions.push(refreshCommand);

  const openNoteCommand = vscode.commands.registerCommand(
    'gnomeClipNotes.openNote',
    async (target?: number | NoteNode) => {
      let noteId: number | undefined;
      if (typeof target === 'number') {
        noteId = target;
      } else if (target instanceof NoteNode) {
        noteId = target.item.id;
      }

      if (noteId === undefined) {
        // If run from command palette without argument, show folder & note quick pick
        try {
          if (!bridgeClient) {
            return;
          }
          const folders = await bridgeClient.listFolders(false);
          const allItems = await Promise.all(
            folders.map(async (f) => {
              const items = await bridgeClient!.listItems(f.id);
              return items.map((it) => ({
                label: it.title || `Note #${it.id}`,
                description: `Folder: ${f.name}`,
                id: it.id
              }));
            })
          );
          const flatItems = allItems.flat();
          if (flatItems.length === 0) {
            vscode.window.showInformationMessage('No notes found in GnomeClipNotes.');
            return;
          }
          const picked = await vscode.window.showQuickPick(flatItems, {
            placeHolder: 'Select a note to open'
          });
          if (picked) {
            noteId = picked.id;
          } else {
            return;
          }
        } catch (err: any) {
          vscode.window.showErrorMessage(`Failed to list notes: ${err.message || String(err)}`);
          return;
        }
      }

      try {
        await projectionManager?.openNote(noteId);
      } catch (err: any) {
        vscode.window.showErrorMessage(`Failed to open note #${noteId}: ${err.message || String(err)}`);
      }
    }
  );
  context.subscriptions.push(openNoteCommand);

  const clearCacheCommand = vscode.commands.registerCommand(
    'gnomeClipNotes.clearCache',
    () => {
      projectionManager?.clearCache();
      vscode.window.showInformationMessage('GnomeClipNotes cache cleared successfully.');
    }
  );
  context.subscriptions.push(clearCacheCommand);

  // Configuration listener
  const configListener = vscode.workspace.onDidChangeConfiguration((e) => {
    if (e.affectsConfiguration('gnomeClipNotes.bridgePath')) {
      const newBridgePath = vscode.workspace
        .getConfiguration('gnomeClipNotes')
        .get<string>('bridgePath', 'gnome-clip-notes');
      bridgeClient?.setBridgePath(newBridgePath);
      treeDataProvider?.refresh();
    }
    if (e.affectsConfiguration('gnomeClipNotes.debounceMs')) {
      const newDebounce = vscode.workspace
        .getConfiguration('gnomeClipNotes')
        .get<number>('debounceMs', 250);
      projectionManager?.setDebounceMs(newDebounce);
    }
  });
  context.subscriptions.push(configListener);
}

export function deactivate(): void {
  try {
    projectionManager?.deactivate();
  } catch (err) {
    console.error('Error during projectionManager deactivation:', err);
  } finally {
    projectionManager = null;
  }

  try {
    bridgeClient?.dispose();
  } catch (err) {
    console.error('Error during bridgeClient disposal:', err);
  } finally {
    bridgeClient = null;
  }

  treeDataProvider = null;
}

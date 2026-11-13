import * as vscode from 'vscode';
import { BridgeClient } from './bridgeClient';
import { Folder, ItemSummary } from './types';

export type TreeNode = FolderNode | NoteNode;

export class FolderNode {
  constructor(public readonly folder: Folder) {}
}

export class NoteNode {
  constructor(public readonly item: ItemSummary) {}
}

export class GnomeClipNotesTreeDataProvider
  implements vscode.TreeDataProvider<TreeNode>
{
  private _onDidChangeTreeData: vscode.EventEmitter<TreeNode | undefined | void> =
    new vscode.EventEmitter<TreeNode | undefined | void>();
  public readonly onDidChangeTreeData: vscode.Event<TreeNode | undefined | void> =
    this._onDidChangeTreeData.event;

  private bridgeClient: BridgeClient;

  constructor(bridgeClient: BridgeClient) {
    this.bridgeClient = bridgeClient;

    // Refresh tree whenever bridge emits 'changed'
    this.bridgeClient.on('changed', () => {
      this.refresh();
    });
  }

  public refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  public getTreeItem(element: TreeNode): vscode.TreeItem {
    if (element instanceof FolderNode) {
      const folder = element.folder;
      const treeItem = new vscode.TreeItem(
        folder.name,
        vscode.TreeItemCollapsibleState.Collapsed
      );
      treeItem.description = `(${folder.item_count})`;
      treeItem.contextValue = 'folder';
      treeItem.iconPath = new vscode.ThemeIcon('folder');
      return treeItem;
    } else {
      const item = element.item;
      const label = this.getItemLabel(item);
      const treeItem = new vscode.TreeItem(
        label,
        vscode.TreeItemCollapsibleState.None
      );
      treeItem.contextValue = 'note';
      treeItem.iconPath = new vscode.ThemeIcon('file-text');
      treeItem.tooltip = `Note #${item.id} (Updated: ${new Date(item.updated_at * 1000).toLocaleString()})`;
      treeItem.command = {
        command: 'gnomeClipNotes.openNote',
        title: 'Open Note',
        arguments: [item.id]
      };
      return treeItem;
    }
  }

  public async getChildren(element?: TreeNode): Promise<TreeNode[]> {
    if (!element) {
      // Top level: folders
      try {
        const folders = await this.bridgeClient.listFolders(false);
        return folders.map((f) => new FolderNode(f));
      } catch (err: any) {
        vscode.window.showErrorMessage(
          `Failed to load folders from GnomeClipNotes bridge: ${err.message || String(err)}`
        );
        return [];
      }
    }

    if (element instanceof FolderNode) {
      // Folder children: items
      try {
        const items = await this.bridgeClient.listItems(element.folder.id);
        return items.map((it) => new NoteNode(it));
      } catch (err: any) {
        vscode.window.showErrorMessage(
          `Failed to load items for folder '${element.folder.name}': ${err.message || String(err)}`
        );
        return [];
      }
    }

    return [];
  }

  public getItemLabel(item: ItemSummary): string {
    const trimmedTitle = item.title ? item.title.trim() : '';
    if (trimmedTitle) {
      return trimmedTitle;
    }
    return `Note #${item.id}`;
  }
}

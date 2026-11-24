import * as vscode from 'vscode';
import { BridgeClient } from './bridgeClient';
import { Folder, ItemDetails, ItemSummary } from './types';

const MAX_TOOLTIP_COMMENT_CHARS = 2000;
const SHORT_MONTHS = [
  'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
  'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'
];

/**
 * Escapes Markdown syntax and `$(icon)` sequences so user text stays inert in tooltips.
 */
export function escapeMarkdown(text: string): string {
  return text
    .replace(/\r\n?/g, '\n')
    .replace(/[\\`*_{}[\]()#+\-.!|><~$]/g, '\\$&');
}

export function formatTimestamp(unixSeconds: number): string {
  if (!Number.isFinite(unixSeconds) || unixSeconds <= 0) {
    return '';
  }
  const d = new Date(unixSeconds * 1000);
  const month = SHORT_MONTHS[d.getMonth()];
  const day = d.getDate();
  const year = d.getFullYear();
  const hours = String(d.getHours()).padStart(2, '0');
  const minutes = String(d.getMinutes()).padStart(2, '0');
  return `${month} ${day}, ${year} · ${hours}:${minutes}`;
}

export function formatContentLength(charCount: number, wordCount: number): string {
  const charLabel = charCount === 1 ? 'character' : 'characters';
  const wordLabel = wordCount === 1 ? 'word' : 'words';
  return `${charCount} ${charLabel} · ${wordCount} ${wordLabel}`;
}

function formatCommentText(comment: string): string {
  const normalized = comment.replace(/^[\r\n]+|[\r\n]+$/g, '').replace(/\r\n?/g, '\n');
  const chars = Array.from(normalized);
  const clipped =
    chars.length > MAX_TOOLTIP_COMMENT_CHARS
      ? chars.slice(0, MAX_TOOLTIP_COMMENT_CHARS).join('') + '…'
      : normalized;
  return clipped
    .split('\n')
    .map((line) => escapeMarkdown(line))
    .join('  \n');
}

export function buildItemDetailsTooltip(details: ItemDetails): vscode.MarkdownString {
  const rawTitle = details.title.trim();
  const displayTitle =
    rawTitle || (details.kind === 'link' ? 'Link' : `Note #${details.id}`);
  const folderLabel = escapeMarkdown(details.folder_name || 'Notes');
  const pinnedSuffix = details.pinned_at > 0 ? ' · $(pinned) Pinned' : '';

  const sections: string[] = [
    `**${escapeMarkdown(displayTitle)}**  \n$(folder) ${folderLabel}${pinnedSuffix}`
  ];

  const annotationLines: string[] = [];
  if (details.category_name) {
    const category = escapeMarkdown(details.category_name);
    const subcategory = details.subcategory_name
      ? ` · ${escapeMarkdown(details.subcategory_name)}`
      : '';
    annotationLines.push(`$(tag) Category: ${category}${subcategory}`);
  }
  if (details.comment && details.comment.trim()) {
    annotationLines.push(
      `$(comment) Comment:  \n${formatCommentText(details.comment)}`
    );
  }
  if (annotationLines.length > 0) {
    sections.push(annotationLines.join('\n\n'));
  }

  const trimmedSource = details.source.trim();
  const baseOrigin = details.origin === 'manual' ? 'Note' : 'Clipboard';
  const originGroup = trimmedSource
    ? `${baseOrigin} · ${escapeMarkdown(trimmedSource)}`
    : baseOrigin;

  const dateLines: string[] = [
    `Created: ${escapeMarkdown(formatTimestamp(details.created_at))}`
  ];
  if (details.updated_at > 0 && details.updated_at !== details.created_at) {
    dateLines.push(`Edited: ${escapeMarkdown(formatTimestamp(details.updated_at))}`);
  }
  if (details.copied_at > details.created_at) {
    dateLines.push(`Last Copied: ${escapeMarkdown(formatTimestamp(details.copied_at))}`);
  }
  if (details.pinned_at > 0) {
    dateLines.push(`Pinned At: ${escapeMarkdown(formatTimestamp(details.pinned_at))}`);
  }

  const lengthGroup = formatContentLength(details.char_count, details.word_count);
  sections.push([originGroup, dateLines.join('  \n'), lengthGroup].join('\n\n'));

  const md = new vscode.MarkdownString(sections.join('\n\n---\n\n'), true);
  md.isTrusted = false;
  return md;
}

export function buildFallbackTooltip(item: ItemSummary): string {
  const label = item.title?.trim() || `Note #${item.id}`;
  const pinnedPrefix = (item.pinned_at ?? 0) > 0 ? 'Pinned · ' : '';
  return `${label} (${pinnedPrefix}Updated: ${formatTimestamp(item.updated_at)})`;
}

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
      const isPinned = (item.pinned_at ?? 0) > 0;
      const treeItem = new vscode.TreeItem(
        label,
        vscode.TreeItemCollapsibleState.None
      );
      treeItem.contextValue = 'note';
      treeItem.iconPath = new vscode.ThemeIcon(isPinned ? 'pinned' : 'file-text');
      // Leave tooltip undefined here so VS Code calls resolveTreeItem lazily on hover.
      treeItem.command = {
        command: 'gnomeClipNotes.openNote',
        title: 'Open Note',
        arguments: [item.id]
      };
      return treeItem;
    }
  }

  public async resolveTreeItem(
    item: vscode.TreeItem,
    element: TreeNode,
    token?: vscode.CancellationToken
  ): Promise<vscode.TreeItem> {
    if (!(element instanceof NoteNode) || token?.isCancellationRequested) {
      return item;
    }

    try {
      const details = await this.bridgeClient.readItemDetails(element.item.id);
      if (!token?.isCancellationRequested) {
        item.tooltip = buildItemDetailsTooltip(details);
      }
    } catch {
      // Silently fall back when talking to an older 1.6.0 bridge or deleted item.
      if (!token?.isCancellationRequested) {
        item.tooltip = buildFallbackTooltip(element.item);
      }
    }
    return item;
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

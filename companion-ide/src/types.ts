/**
 * Protocol and domain types for GnomeClipNotes IDE Companion.
 */

export interface BridgeRequest {
  request_id: string;
  method: string;
  params?: Record<string, any>;
}

export interface BridgeErrorDetails {
  code:
    | 'INVALID_REQUEST'
    | 'NOT_FOUND'
    | 'CONTENT_TOO_LARGE'
    | 'INVALID_CONTENT'
    | 'CONFLICT'
    | 'FORBIDDEN'
    | 'INTERNAL_ERROR'
    | string;
  message: string;
  details?: any;
}

export interface BridgeSuccessResponse<T = any> {
  type: 'response';
  request_id: string;
  ok: true;
  result: T;
}

export interface BridgeErrorResponse {
  type: 'response';
  request_id: string;
  ok: false;
  error: BridgeErrorDetails;
}

export type BridgeResponse<T = any> = BridgeSuccessResponse<T> | BridgeErrorResponse;

export interface BridgeChangedEvent {
  type: 'event';
  event: 'changed';
  timestamp: number;
}

export type BridgeMessage = BridgeResponse | BridgeChangedEvent;

export interface PingResult {
  version: string;
  max_content: number;
}

export interface Folder {
  id: number;
  name: string;
  item_count: number;
  position: number;
}

export interface ItemSummary {
  id: number;
  folder_id: number;
  title: string;
  updated_at: number;
  created_at: number;
  revision: string;
}

export interface ItemDetail {
  id: number;
  folder_id: number;
  title: string;
  content: string;
  updated_at: number;
  created_at: number;
  revision: string;
}

export interface SaveItemResult {
  id: number;
  revision: string;
  updated_at: number;
}

export interface ConflictDetails {
  current_revision: string;
  title: string;
  content: string;
  updated_at: number;
}

export interface ContentTooLargeDetails {
  max_bytes: number;
  actual_bytes: number;
}

export class BridgeError extends Error {
  public readonly code: string;
  public readonly details?: any;

  constructor(details: BridgeErrorDetails) {
    super(details.message);
    this.name = 'BridgeError';
    this.code = details.code;
    this.details = details.details;
  }
}

export interface TrackedNote {
  id: number;
  folderId: number;
  title: string;
  revision: string;
  filePath: string;
  lastSyncedHash: string;
  lastSyncedAt: number;
}

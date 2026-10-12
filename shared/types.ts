export type RelationType =
  | 'reference'
  | 'derived'
  | 'contradicts'
  | 'supersedes'
  | 'extends'
  | 'custom';

export interface KBInfo {
  id: string;
  name: string;
  path: string;
}

export interface Relation {
  id: string;
  fromId: string;
  toId: string;
  type: RelationType;
  note?: string;
}

export interface Attachment {
  path: string;
  caption?: string;
  type: 'image' | 'document' | 'other';
}

export interface Entry {
  id: string;
  title: string;
  tags: string[];
  labels: string[];
  group?: string;
  path: string;
  contentPath: string;
  attachments: Attachment[];
  relations: Relation[];
  hasContentMd: boolean;
  status?: string;
  errorCount?: number;
  warningCount?: number;
}

/// Shape returned by the backend's `get_entries` /
/// `update_entry_metadata` calls. Mirrors `index::StoredEntry` on the
/// Rust side (camelCase serialized). Has additional fields beyond
/// `Entry` (aliases, source, created, updated, ignoredRules) — the
/// frontend ignores those when reconciling into `Entry`.
export interface StoredEntry {
  id: string;
  title: string;
  path: string;
  group?: string | null;
  tags: string[];
  attachments: Attachment[];
  relations: Relation[];
  hasContentMd: boolean;
  labels: string[];
  status?: string | null;
  aliases: string[];
  source?: string | null;
  created?: string | null;
  updated?: string | null;
  ignoredRules: string[];
  errorCount: number;
  warningCount: number;
}

export interface Layout {
  entryId: string;
  x: number;
  y: number;
}

export interface AppConfig {
  kbRoot: string | null;
  relationColors: Record<RelationType, string>;
  theme: 'light' | 'dark';
}

// ---------- Cycle 2: inline metadata editing + rename/move ----------

/// Field changes passed from the frontend. `null` or missing means
/// "leave alone". Mirrors `parser::metadata_updater::MetadataChanges`
/// on the Rust side (camelCase serialized).
export interface MetadataChanges {
  title?: string | null;
  tags?: string[] | null;
  labels?: string[] | null;
  status?: string | null;
}

/// Result of a pure-read rename precheck. The dialog calls
/// `precheck_rename` on every keystroke (debounced) to drive the
/// validation UI. No side effects. Mirrors
/// `commands::PrecheckRenameResult` on the Rust side.
export interface PrecheckRenameResult {
  valid: boolean;
  newId: string;
  conflictWith: string | null;
  incomingCount: number;
  incomingFiles: string[];
}

/// Result of executing a rename. `newId` is the entry's new id; the
/// frontend should re-select by `newId` if the renamed entry was
/// selected. Mirrors `commands::RenameResult` on the Rust side.
export interface RenameResult {
  newId: string;
  updatedFiles: string[];
}

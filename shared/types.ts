export type RelationType =
  | 'reference'
  | 'derived'
  | 'contradicts'
  | 'supersedes'
  | 'extends'
  | 'custom';

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
  group?: string;
  path: string;
  contentPath: string;
  attachments: Attachment[];
  relations: Relation[];
  hasContentMd: boolean;
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

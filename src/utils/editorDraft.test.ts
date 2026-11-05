import { test } from 'node:test';
import assert from 'node:assert/strict';

import {
  clearDraft,
  draftKey,
  pendingDraft,
  readDraft,
  writeDraft,
  type DraftStorage,
} from './editorDraft.ts';

function memoryStorage(): DraftStorage {
  const map = new Map<string, string>();
  return {
    getItem: (k) => map.get(k) ?? null,
    setItem: (k, v) => {
      map.set(k, v);
    },
    removeItem: (k) => {
      map.delete(k);
    },
  };
}

test('draft key isolates kb and entry', () => {
  assert.equal(draftKey('kb1', 'note'), 'synapse:draft:kb1:note');
  assert.equal(draftKey(null, 'note'), 'synapse:draft:_:note');
});

test('write/read roundtrip and clear', () => {
  const storage = memoryStorage();
  writeDraft('kb1', 'note', '未保存的内容', storage, 1234);

  const draft = readDraft('kb1', 'note', storage);
  assert.deepEqual(draft, { content: '未保存的内容', updatedAt: 1234 });

  clearDraft('kb1', 'note', storage);
  assert.equal(readDraft('kb1', 'note', storage), null);
});

test('invalid payload is ignored instead of crashing', () => {
  const storage = memoryStorage();
  const key = draftKey('kb1', 'note');
  storage.setItem(key, '{"content":123}');
  assert.equal(readDraft('kb1', 'note', storage), null);

  storage.setItem(key, 'not json');
  assert.equal(readDraft('kb1', 'note', storage), null);
});

test('pendingDraft only returns drafts that differ from saved content', () => {
  const storage = memoryStorage();
  writeDraft('kb1', 'note', 'same', storage, 1);
  assert.equal(pendingDraft('kb1', 'note', 'same', storage), null);
  assert.deepEqual(pendingDraft('kb1', 'note', 'other', storage), {
    content: 'same',
    updatedAt: 1,
  });
});

test('a storage that throws does not break editing', () => {
  const broken: DraftStorage = {
    getItem: () => {
      throw new Error('blocked');
    },
    setItem: () => {
      throw new Error('quota');
    },
    removeItem: () => {
      throw new Error('blocked');
    },
  };
  assert.equal(readDraft('kb1', 'note', broken), null);
  assert.doesNotThrow(() => writeDraft('kb1', 'note', 'x', broken));
  assert.doesNotThrow(() => clearDraft('kb1', 'note', broken));
});

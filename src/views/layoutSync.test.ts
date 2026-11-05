import assert from 'node:assert/strict';
import { test } from 'node:test';

import { mergeLayoutPositions } from './layoutSync';

test('空布局不改变内存坐标', () => {
  const prev = { a: { x: 10, y: 20 } };
  assert.equal(mergeLayoutPositions(prev, null), prev);
  assert.deepEqual(mergeLayoutPositions(prev, {}), prev);
});

test('磁盘上的新键被合并进来（重命名后补上新 id）', () => {
  const prev = { 'old-id': { x: 10, y: 20 } };
  const loaded = { 'new-id': { x: 10, y: 20 } };
  const merged = mergeLayoutPositions(prev, loaded);
  assert.deepEqual(merged['new-id'], { x: 10, y: 20 });
});

test('磁盘值覆盖同键内存值，未落盘的键保留内存值', () => {
  const prev = {
    a: { x: 1, y: 2 },
    dragging: { x: 99, y: 99 },
  };
  const loaded = { a: { x: 5, y: 6 } };
  const merged = mergeLayoutPositions(prev, loaded);
  assert.deepEqual(merged.a, { x: 5, y: 6 });
  assert.deepEqual(merged.dragging, { x: 99, y: 99 });
});

test('不修改传入对象（便于 React 状态比较）', () => {
  const prev = { a: { x: 1, y: 2 } };
  const loaded = { b: { x: 3, y: 4 } };
  mergeLayoutPositions(prev, loaded);
  assert.deepEqual(prev, { a: { x: 1, y: 2 } });
});

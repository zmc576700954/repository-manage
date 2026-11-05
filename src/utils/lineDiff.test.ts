import { test } from 'node:test';
import assert from 'node:assert/strict';

import { changedLinePairs } from './lineDiff.ts';

test('returns only lines that actually changed, with 1-based numbers', () => {
  const before = '#  标题  \n\n-   item\n\n正文   \n';
  const after = '# 标题\n\n- item\n\n正文\n';
  assert.deepEqual(changedLinePairs(before, after), [
    { lineNo: 1, before: '#  标题  ', after: '# 标题' },
    { lineNo: 3, before: '-   item', after: '- item' },
    { lineNo: 5, before: '正文   ', after: '正文' },
  ]);
});

test('identical content yields no changes', () => {
  assert.deepEqual(changedLinePairs('a\nb\n', 'a\nb\n'), []);
});

test('tolerates line-count differences by treating missing lines as empty', () => {
  assert.deepEqual(changedLinePairs('a\n', 'a\nb\n'), [{ lineNo: 2, before: '', after: 'b' }]);
});

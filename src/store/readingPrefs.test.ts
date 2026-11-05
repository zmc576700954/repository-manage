import assert from 'node:assert/strict';
import test from 'node:test';
import {
  DEFAULT_READING_PREFS,
  normalizeReadingPrefs,
} from './readingPrefsStore.ts';

test('缺省值来自默认参数', () => {
  assert.deepEqual(normalizeReadingPrefs(undefined), DEFAULT_READING_PREFS);
  assert.deepEqual(normalizeReadingPrefs({}), DEFAULT_READING_PREFS);
});

test('越界参数被收敛到合法区间', () => {
  const prefs = normalizeReadingPrefs({
    fontSize: 99,
    lineHeight: 0.5,
    paragraphGap: 9,
    codeFontSize: 2,
  });
  assert.equal(prefs.fontSize, 20);
  assert.equal(prefs.lineHeight, 1.4);
  assert.equal(prefs.paragraphGap, 1.4);
  assert.equal(prefs.codeFontSize, 11);
});

test('最大行宽 0 表示不限制，其余值收敛', () => {
  assert.equal(normalizeReadingPrefs({ maxWidth: 0 }).maxWidth, 0);
  assert.equal(normalizeReadingPrefs({ maxWidth: 20 }).maxWidth, 50);
  assert.equal(normalizeReadingPrefs({ maxWidth: 500 }).maxWidth, 110);
});

test('表格样式只接受已知值', () => {
  assert.equal(normalizeReadingPrefs({ tableStyle: 'striped' }).tableStyle, 'striped');
  assert.equal(
    normalizeReadingPrefs({ tableStyle: 'weird' as never }).tableStyle,
    'bordered'
  );
});

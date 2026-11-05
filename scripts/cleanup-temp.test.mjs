import assert from 'node:assert/strict';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import { cleanupStaleTempDirs, DEFAULT_MAX_AGE_MS, TEMP_PREFIX } from './cleanup-temp.mjs';

function makeRoot() {
  const root = mkdtempSync(join(tmpdir(), 'cleanup-test-'));
  return root;
}

test('只删除“本项目前缀 + 真实目录 + 超过年龄阈值”的条目', () => {
  const root = makeRoot();
  try {
    const now = Date.now();
    const stale = join(root, `${TEMP_PREFIX}stale`);
    const fresh = join(root, `${TEMP_PREFIX}fresh`);
    const other = join(root, 'someone-elses-dir');
    const file = join(root, `${TEMP_PREFIX}file`);
    const link = join(root, `${TEMP_PREFIX}link`);
    mkdirSync(stale);
    mkdirSync(fresh);
    mkdirSync(other);
    writeFileSync(file, 'x');
    symlinkSync(other, link);

    // 把“旧目录”的 mtime 拨回 2 个阈值之前。
    const old = new Date(now - DEFAULT_MAX_AGE_MS * 2);
    utimesSync(stale, old, old);
    utimesSync(other, old, old);
    utimesSync(file, old, old);

    const removed = cleanupStaleTempDirs({ root, now });
    assert.equal(removed, 1);

    assert.equal(existsSync(stale), false, 'stale project dir must be removed');
    assert.equal(existsSync(fresh), true, 'fresh dir must be kept');
    assert.equal(existsSync(other), true, 'unrelated dir must be kept');
    assert.equal(existsSync(file), true, 'plain files must be kept');
    assert.equal(existsSync(link), true, 'symlinks must be kept');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('根目录不存在或不可读时返回 0，不抛异常', () => {
  const missing = join(tmpdir(), 'definitely-not-here-synapse');
  assert.equal(cleanupStaleTempDirs({ root: missing }), 0);
});

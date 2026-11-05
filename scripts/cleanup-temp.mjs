/**
 * 清理被硬杀（SIGKILL / 崩溃 / 断电）后残留在系统临时目录里的单测工作目录。
 *
 * 安全边界（避免误删他人数据）：
 * 1. 只在 `os.tmpdir()` 下操作，不接受调用方传入任意根目录以外的位置；
 * 2. 目录名必须以本项目专属前缀 `synapse-unit-` 开头；
 * 3. 必须是**真实目录**：symlink、普通文件、其他类型一律跳过（不跟随链接）；
 * 4. 只删除 mtime 超过阈值的目录（默认 24h），刚创建/仍在写入的目录不碰；
 * 5. 任何 readdir/lstat/rm 失败都忽略，清理永不阻塞测试运行。
 */
import { readdirSync, lstatSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export const TEMP_PREFIX = 'synapse-unit-';
export const DEFAULT_MAX_AGE_MS = 24 * 60 * 60 * 1000;

export function cleanupStaleTempDirs({
  root = tmpdir(),
  prefix = TEMP_PREFIX,
  maxAgeMs = DEFAULT_MAX_AGE_MS,
  now = Date.now(),
  log = () => {},
} = {}) {
  let removed = 0;
  let entries;
  try {
    entries = readdirSync(root, { withFileTypes: true });
  } catch {
    return 0;
  }
  for (const entry of entries) {
    if (!entry.name.startsWith(prefix)) continue;
    // Dirent 与 lstat 双重校验：不跟随 symlink，也不碰非目录。
    if (!entry.isDirectory() || entry.isSymbolicLink()) continue;
    const full = join(root, entry.name);
    let stat;
    try {
      stat = lstatSync(full);
    } catch {
      continue;
    }
    if (!stat.isDirectory() || stat.isSymbolicLink()) continue;
    if (now - stat.mtimeMs < maxAgeMs) continue;
    try {
      rmSync(full, { recursive: true, force: true });
      removed += 1;
      log(full);
    } catch {
      // 清理失败无所谓，下一次启动再试。
    }
  }
  return removed;
}

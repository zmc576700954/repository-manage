/**
 * 跨平台的前端单测运行器：用 Node 的临时目录 API 生成测试产物，跑完
 * （无论成功失败）都在 finally 里清理，不再往固定的 /tmp 路径写东西。
 *
 * 进程被硬杀时 `finally` 不会执行，因此每次启动先清理过期的
 * `synapse-unit-*` 目录（前缀 + 24h 年龄，具体安全边界见 cleanup-temp.mjs）。
 *
 * 用法：`npm run test:unit`（CI 可直接 `npm test`）。
 */
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { cleanupStaleTempDirs } from './cleanup-temp.mjs';

cleanupStaleTempDirs({ log: (dir) => console.warn(`[test] removed stale temp dir: ${dir}`) });

const workDir = mkdtempSync(join(tmpdir(), 'synapse-unit-'));
const buildDir = join(workDir, 'build');
const tsTests = [
  'src/events/kbRefresh.test.ts',
  'src/views/layoutSync.test.ts',
  'src/utils/editorDraft.test.ts',
  'src/utils/lineDiff.test.ts',
];
const plainTests = ['scripts/cleanup-temp.test.mjs'];
const isWindows = process.platform === 'win32';
const esbuildBin = join('node_modules', '.bin', isWindows ? 'esbuild.cmd' : 'esbuild');

let exitCode = 1;
try {
  const build = spawnSync(
    esbuildBin,
    [
      ...tsTests,
      '--bundle',
      '--format=esm',
      '--platform=node',
      `--outdir=${buildDir}`,
      '--log-level=warning',
    ],
    { stdio: 'inherit', shell: isWindows },
  );

  if (build.error || build.status !== 0) {
    exitCode = build.status ?? 1;
  } else {
    const run = spawnSync(process.execPath, ['--test', buildDir, ...plainTests], {
      stdio: 'inherit',
    });
    exitCode = run.status ?? 1;
  }
} finally {
  rmSync(workDir, { recursive: true, force: true });
}

process.exitCode = exitCode;

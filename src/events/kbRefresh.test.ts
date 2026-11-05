import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  createKbRefreshCoordinator,
  type KbChangedPayload,
} from './kbRefresh';

interface FakeClock {
  now: () => number;
  schedule: (fn: () => void, ms: number) => unknown;
  cancel: (handle: unknown) => void;
  advance: (ms: number) => void;
}

function makeClock(): FakeClock {
  let t = 10_000;
  let seq = 0;
  const timers = new Map<number, { at: number; fn: () => void }>();
  return {
    now: () => t,
    schedule: (fn, ms) => {
      const id = ++seq;
      timers.set(id, { at: t + ms, fn });
      return id;
    },
    cancel: (handle) => {
      timers.delete(handle as number);
    },
    advance: (ms) => {
      t += ms;
      for (;;) {
        const due = [...timers.entries()]
          .filter(([, timer]) => timer.at <= t)
          .sort((a, b) => a[1].at - b[1].at);
        if (due.length === 0) return;
        for (const [id, timer] of due) {
          if (!timers.delete(id)) continue;
          timer.fn();
        }
      }
    },
  };
}

function payload(overrides: Partial<KbChangedPayload> = {}): KbChangedPayload {
  return {
    kbId: 'kb1',
    entryCount: 1,
    added: [],
    removed: [],
    renamed: [],
    origin: 'external',
    fingerprint: 'fp-0',
    revision: 1,
    ...overrides,
  };
}

function makeCoordinator(
  options: {
    applied?: KbChangedPayload[];
    busy?: () => boolean;
    synced?: () => string | null;
  } = {},
) {
  const clock = makeClock();
  const applied = options.applied ?? [];
  const coordinator = createKbRefreshCoordinator({
    getActiveKbId: () => 'kb1',
    isBusy: options.busy ?? (() => false),
    apply: (p) => applied.push(p),
    getSyncedFingerprint: options.synced ?? (() => null),
    schedule: clock.schedule,
    cancel: clock.cancel,
  });
  return { clock, applied, coordinator };
}

test('防抖：窗口内的多次事件只刷新一次，并合并 added/removed/renamed', () => {
  const { clock, applied, coordinator } = makeCoordinator();

  coordinator.handle(payload({ fingerprint: 'fp-a', added: ['a'] }));
  clock.advance(100);
  coordinator.handle(
    payload({
      fingerprint: 'fp-b',
      added: ['b'],
      removed: ['x'],
      renamed: [{ fromId: 'old-1', toId: 'new-1' }],
    }),
  );
  clock.advance(100);
  assert.equal(applied.length, 0, '静默窗口内不得刷新');

  clock.advance(200);
  assert.equal(applied.length, 1, '窗口结束后只刷新一次');
  assert.equal(applied[0].fingerprint, 'fp-b', '以最后一次负载为准');
  assert.deepEqual(applied[0].added, ['a', 'b'], 'added 应取并集');
  assert.deepEqual(applied[0].removed, ['x']);
  assert.deepEqual(
    applied[0].renamed,
    [{ fromId: 'old-1', toId: 'new-1' }],
    'renamed 应保留增量信息',
  );
  assert.equal(coordinator.hasPending(), false);
});

test('指纹去重：已应用的负载再次到达时不重复刷新', () => {
  const { clock, applied, coordinator } = makeCoordinator();

  coordinator.handle(payload({ fingerprint: 'fp-a' }));
  clock.advance(300);
  assert.equal(applied.length, 1);

  coordinator.handle(payload({ fingerprint: 'fp-a' }));
  clock.advance(300);
  assert.equal(applied.length, 1, '相同指纹不重复刷新');
});

test('自身写操作回声：origin=local-write 且指纹与已同步一致时丢弃', () => {
  let synced: string | null = 'fp-self';
  const { clock, applied, coordinator } = makeCoordinator({
    synced: () => synced,
  });

  coordinator.handle(
    payload({ fingerprint: 'fp-self', origin: 'local-write', added: ['mine'] }),
  );
  clock.advance(300);
  assert.equal(applied.length, 0, '自己已同步的内容不需要再刷');

  // 同一指纹的迟到事件仍然忽略（已记住该指纹）。
  clock.advance(1500);
  coordinator.handle(
    payload({ fingerprint: 'fp-self', origin: 'local-write' }),
  );
  clock.advance(300);
  assert.equal(applied.length, 0);

  // 换成外部变更：指纹不同，必须刷新。
  synced = null;
  coordinator.handle(payload({ fingerprint: 'fp-ext', origin: 'external' }));
  clock.advance(300);
  assert.equal(applied.length, 1);
});

test('外部与 UI 写同批：local-write 但指纹与本地已同步不同 → 照常刷新', () => {
  const { clock, applied, coordinator } = makeCoordinator({
    synced: () => 'fp-after-local',
  });

  // UI 刚写完，watcher 批次里同时包含外部改动，指纹与本地已同步的不同。
  coordinator.handle(
    payload({
      fingerprint: 'fp-mixed',
      origin: 'local-write',
      added: ['external-entry'],
    }),
  );
  clock.advance(300);
  assert.equal(applied.length, 1, '合并了外部变更的回声不得丢弃');
  assert.deepEqual(applied[0].added, ['external-entry']);
});

test('外部变更不再受时间窗影响：紧跟在本地写之后到达也会刷新', () => {
  let synced: string | null = 'fp-local';
  const { clock, applied, coordinator } = makeCoordinator({
    synced: () => synced,
  });

  synced = 'fp-local';
  coordinator.handle(
    payload({ fingerprint: 'fp-external', origin: 'external' }),
  );
  // 立刻结束防抖窗口即可刷新（无需等待任何 1.5s 自身写窗口）。
  clock.advance(300);
  assert.equal(applied.length, 1);
  assert.equal(applied[0].fingerprint, 'fp-external');
});

test('忙碌保护：重命名/拖拽进行中延迟刷新，结束后只补一次', () => {
  let busy = true;
  const { clock, applied, coordinator } = makeCoordinator({
    busy: () => busy,
  });

  coordinator.handle(payload({ fingerprint: 'fp-1' }));
  clock.advance(300);
  assert.equal(applied.length, 0, '交互中不得刷新');
  assert.equal(coordinator.hasPending(), true, '事件应被保留而不是丢弃');

  busy = false;
  clock.advance(300);
  assert.equal(applied.length, 1, '交互结束后补刷一次');
});

test('非当前 KB 的事件直接忽略', () => {
  const { clock, applied, coordinator } = makeCoordinator();

  coordinator.handle(payload({ kbId: 'kb2', fingerprint: 'fp-2' }));
  clock.advance(300);
  assert.equal(applied.length, 0);
});

test('切换 KB 后丢弃旧 KB 的待办事件', () => {
  const clock = makeClock();
  const applied: KbChangedPayload[] = [];
  let active = 'kb1';
  const coordinator = createKbRefreshCoordinator({
    getActiveKbId: () => active,
    isBusy: () => false,
    apply: (p) => applied.push(p),
    getSyncedFingerprint: () => null,
    schedule: clock.schedule,
    cancel: clock.cancel,
  });

  coordinator.handle(payload({ fingerprint: 'fp-1' }));
  active = 'kb2';
  clock.advance(300);
  assert.equal(applied.length, 0, '旧 KB 事件不得刷进新 KB');
  assert.equal(coordinator.hasPending(), false);
});

test('dispose 会取消未执行的刷新', () => {
  const { clock, applied, coordinator } = makeCoordinator();

  coordinator.handle(payload({ fingerprint: 'fp-1' }));
  coordinator.dispose();
  clock.advance(1000);
  assert.equal(applied.length, 0);
});

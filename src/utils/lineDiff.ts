/**
 * 一键排版的预览差异：排版规则是逐行、行数不变的，所以按行号逐行比较即可，
 * 不需要引入 diff 库。
 */

export interface LineChange {
  /// 行号（从 1 开始），用于界面上定位。
  lineNo: number;
  before: string;
  after: string;
}

export function changedLinePairs(before: string, after: string): LineChange[] {
  const b = before.split('\n');
  const a = after.split('\n');
  const max = Math.max(b.length, a.length);
  const out: LineChange[] = [];
  for (let i = 0; i < max; i += 1) {
    const beforeLine = b[i] ?? '';
    const afterLine = a[i] ?? '';
    if (beforeLine !== afterLine) {
      out.push({ lineNo: i + 1, before: beforeLine, after: afterLine });
    }
  }
  return out;
}

// ---------- 批次 4：提案审阅的行级 diff（LCS） ----------

export interface DiffLine {
  type: 'same' | 'add' | 'del';
  text: string;
}

const MAX_DP_LINES = 1500;

/**
 * 提案审阅视图用：当前全文 vs 提案全文的统一视图（LCS 行级 diff）。
 * 条目正文量级小，O(n·m) DP 足够；超长文件退化为「公共前缀/后缀 +
 * 中段整体替换」，保证 UI 不被二次方复杂度拖死。
 */
export function lineDiff(before: string, after: string): DiffLine[] {
  const a = before.split('\n');
  const b = after.split('\n');

  if (a.length > MAX_DP_LINES || b.length > MAX_DP_LINES) {
    let prefix = 0;
    while (prefix < a.length && prefix < b.length && a[prefix] === b[prefix]) prefix += 1;
    let suffix = 0;
    while (
      suffix < a.length - prefix &&
      suffix < b.length - prefix &&
      a[a.length - 1 - suffix] === b[b.length - 1 - suffix]
    )
      suffix += 1;
    return [
      ...a.slice(0, prefix).map((text): DiffLine => ({ type: 'same', text })),
      ...a.slice(prefix, a.length - suffix).map((text): DiffLine => ({ type: 'del', text })),
      ...b.slice(prefix, b.length - suffix).map((text): DiffLine => ({ type: 'add', text })),
      ...a.slice(a.length - suffix).map((text): DiffLine => ({ type: 'same', text })),
    ];
  }

  const dp: Uint32Array[] = Array.from({ length: a.length + 1 }, () =>
    new Uint32Array(b.length + 1)
  );
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      dp[i][j] =
        a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      out.push({ type: 'same', text: a[i] });
      i += 1;
      j += 1;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      out.push({ type: 'del', text: a[i] });
      i += 1;
    } else {
      out.push({ type: 'add', text: b[j] });
      j += 1;
    }
  }
  while (i < a.length) {
    out.push({ type: 'del', text: a[i] });
    i += 1;
  }
  while (j < b.length) {
    out.push({ type: 'add', text: b[j] });
    j += 1;
  }
  return out;
}

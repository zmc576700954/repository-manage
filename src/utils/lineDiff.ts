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

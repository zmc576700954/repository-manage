/**
 * 标准模版字头的纯逻辑：定位正文起点、判断 front matter 冲突、插入而不覆盖。
 *
 * 约定：
 * - 字头是一段可配置的 Markdown（设置项 `templateHeader`）；
 * - 若字头自身以 `---` 包裹的 YAML front matter 开头，而目标文档已有
 *   front matter，则拒绝插入并返回 `conflict`，绝不静默替换；
 * - 普通 Markdown 字头插到正文起点（front matter 与开头的元数据注释之后），
 *   不动已有内容；
 * - 支持 `{{title}}` / `{{date}}` 两个占位符。
 */

export type TemplateInsertStatus =
  | 'inserted'
  | 'already-present'
  | 'conflict'
  | 'empty';

export interface TemplateInsertResult {
  content: string;
  status: TemplateInsertStatus;
}

export interface TemplateContext {
  title: string;
  /** ISO 日期（YYYY-MM-DD）。 */
  date: string;
}

function normalizeNewlines(text: string): string {
  return text.replace(/\r\n?/g, '\n');
}

function isCommentLine(line: string): boolean {
  const trimmed = line.trim();
  return trimmed.startsWith('<!--') && trimmed.endsWith('-->');
}

interface FrontMatterRange {
  /** 起始行号（含 `---`） */
  startLine: number;
  /** 结束行号（含 `---`） */
  endLine: number;
}

/** 与 Rust `extract_yaml_frontmatter` 一致：找第一个 `---` 行及其闭合行。 */
export function findFrontMatter(lines: string[]): FrontMatterRange | null {
  const startLine = lines.findIndex((line) => line.trim() === '---');
  if (startLine < 0) return null;
  for (let i = startLine + 1; i < lines.length; i += 1) {
    if (lines[i].trim() === '---') {
      return { startLine, endLine: i };
    }
  }
  return null;
}

/** 字头自身是否是一个 YAML front matter 块。 */
export function isFrontMatterTemplate(header: string): boolean {
  const lines = normalizeNewlines(header).split('\n');
  if (lines[0]?.trim() !== '---') return false;
  return lines.slice(1).some((line) => line.trim() === '---');
}

/**
 * 正文起点（行号）：跳过 front matter，再跳过开头的空行与元数据注释行。
 * 返回 `lines.length` 表示整个文档只有 front matter/注释。
 */
export function bodyStartLine(lines: string[]): number {
  const fm = findFrontMatter(lines);
  let i = fm ? fm.endLine + 1 : 0;
  while (i < lines.length && (lines[i].trim() === '' || isCommentLine(lines[i]))) {
    i += 1;
  }
  return i;
}

function applyPlaceholders(header: string, ctx: TemplateContext): string {
  return header
    .replace(/\{\{\s*title\s*\}\}/g, ctx.title)
    .replace(/\{\{\s*date\s*\}\}/g, ctx.date);
}

/** 去掉字头结尾多余空行，保留单个换行结尾。 */
function normalizeHeaderBlock(header: string): string {
  const trimmed = header.replace(/\s+$/u, '');
  return `${trimmed}\n`;
}

/**
 * 把字头插入文档。返回新内容与状态；`status !== 'inserted'` 时 `content`
 * 与输入完全相同，调用方应把状态转成可见提示。
 */
export function insertTemplateHeader(
  content: string,
  header: string,
  ctx: TemplateContext
): TemplateInsertResult {
  const raw = normalizeNewlines(header).trim();
  if (raw === '') {
    return { content, status: 'empty' };
  }

  const doc = normalizeNewlines(content);
  const lines = doc.split('\n');
  const existingFm = findFrontMatter(lines);

  if (isFrontMatterTemplate(raw) && existingFm) {
    return { content, status: 'conflict' };
  }

  const block = normalizeHeaderBlock(applyPlaceholders(raw, ctx));
  const pos = isFrontMatterTemplate(raw)
    ? 0
    : lineOffset(lines, Math.min(bodyStartLine(lines), lines.length));
  const before = doc.slice(0, pos);
  const after = doc.slice(pos);

  // 已有相同字头：只比较正文起点开始的一个块，避免把行尾差异当成不同内容。
  const existingBlock = `${after.split('\n').slice(0, block.split('\n').length).join('\n')}\n`;
  if (existingBlock.trimEnd() === block.trimEnd()) {
    return { content, status: 'already-present' };
  }

  let head = before;
  if (head.length > 0 && !head.endsWith('\n')) head += '\n';
  // 字头与正文之间留一个空行。
  const tail = after.replace(/^\n+/, '');
  const next = `${head}${block}${tail ? `\n${tail}` : ''}`;
  return { content: next, status: 'inserted' };
}

/** 行号 → 字节偏移（`lines` 由 `split('\n')` 得到）。 */
function lineOffset(lines: string[], line: number): number {
  let offset = 0;
  for (let i = 0; i < line; i += 1) {
    offset += lines[i].length + 1;
  }
  return Math.min(offset, lines.join('\n').length);
}

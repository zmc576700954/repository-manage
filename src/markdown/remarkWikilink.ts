import { visit, SKIP } from 'unist-util-visit';

/**
 * remark 插件：扫描文本节点中的 [[id]] 或 [[id|caption]]，
 * 替换为自定义 wikilink 节点。
 *
 * 节点结构：
 *   { type: 'wikilink', data: { id: string, caption: string } }
 *
 * `id` 为 entry id（不含 `|caption` 后缀）。
 *   caption 为可选的别名；未指定时与 id 相同。
 *
 * 不处理 code/inlineCode 容器内的文本（不递归）。
 */
type TextNode = { type: 'text'; value: string };
type ParentNode = { type: string; children: any[] };

const WIKILINK_RE = /\[\[([^\[\]\|]+)(?:\|([^\]]+))?\]\]/g;

function isTextNode(n: any): n is TextNode {
  return n && n.type === 'text' && typeof n.value === 'string';
}

function isCodeLike(type: string): boolean {
  return type === 'code' || type === 'inlineCode' || type === 'codeBlock';
}

export function remarkWikilink() {
  return (tree: any) => {
    visit(tree, (node: any, index, parent: ParentNode | undefined) => {
      if (!isTextNode(node)) return;
      // Skip if inside code-like containers
      if (parent && isCodeLike(parent.type)) return;

      const value = node.value;
      WIKILINK_RE.lastIndex = 0;
      const matches: { start: number; end: number; id: string; caption: string }[] = [];
      let m: RegExpExecArray | null;
      while ((m = WIKILINK_RE.exec(value))) {
        const id = m[1];
        const caption = m[2] ?? id;
        matches.push({ start: m.index, end: m.index + m[0].length, id, caption });
      }
      if (matches.length === 0) return;

      const out: any[] = [];
      let cursor = 0;
      for (const m of matches) {
        if (m.start > cursor) {
          out.push({ type: 'text', value: value.slice(cursor, m.start) });
        }
        out.push({
          type: 'wikilink',
          data: { id: m.id, caption: m.caption },
          children: [{ type: 'text', value: m.caption }],
        });
        cursor = m.end;
      }
      if (cursor < value.length) {
        out.push({ type: 'text', value: value.slice(cursor) });
      }

      if (parent && typeof index === 'number') {
        parent.children.splice(index, 1, ...out);
        return [SKIP, index + out.length];
      }
    });
  };
}

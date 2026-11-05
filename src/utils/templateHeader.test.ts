import assert from 'node:assert/strict';
import test from 'node:test';
import {
  bodyStartLine,
  findFrontMatter,
  insertTemplateHeader,
  isFrontMatterTemplate,
} from './templateHeader.ts';

const ctx = { title: '测试文档', date: '2026-11-05' };

test('普通字头插到正文开头', () => {
  const result = insertTemplateHeader('正文第一行\n', '> 记录于 {{date}}', ctx);
  assert.equal(result.status, 'inserted');
  assert.equal(result.content, '> 记录于 2026-11-05\n\n正文第一行\n');
});

test('字头不覆盖已有 front matter，插在其后', () => {
  const original = '---\ntags: [a]\n---\n\n正文\n';
  const result = insertTemplateHeader(original, '# 字头', ctx);
  assert.equal(result.status, 'inserted');
  assert.match(result.content, /^---\ntags: \[a\]\n---\n\n# 字头\n\n正文\n$/);
});

test('开头的元数据注释之后插入普通字头', () => {
  const original = '<!-- @synapse-id: demo -->\n<!-- @synapse-tags: x -->\n\n正文\n';
  const result = insertTemplateHeader(original, '> 字头', ctx);
  assert.equal(result.status, 'inserted');
  assert.match(result.content, /^<!-- @synapse-id: demo -->\n<!-- @synapse-tags: x -->\n\n> 字头\n\n正文\n$/);
});

test('front matter 模版与已有 front matter 冲突时拒绝并保持原文', () => {
  const original = '---\ntags: [a]\n---\n\n正文\n';
  const result = insertTemplateHeader(original, '---\nstatus: draft\n---', ctx);
  assert.equal(result.status, 'conflict');
  assert.equal(result.content, original);
});

test('无 front matter 时把 YAML 字头放在文件最前面', () => {
  const result = insertTemplateHeader('# 正文\n', '---\nstatus: draft\n---', ctx);
  assert.equal(result.status, 'inserted');
  assert.equal(result.content, '---\nstatus: draft\n---\n\n# 正文\n');
});

test('相同字头不重复插入', () => {
  const once = insertTemplateHeader('正文\n', '> 字头', ctx);
  assert.equal(once.status, 'inserted');
  const twice = insertTemplateHeader(once.content, '> 字头', ctx);
  assert.equal(twice.status, 'already-present');
  assert.equal(twice.content, once.content);
});

test('空字头返回 empty 且不改内容', () => {
  const result = insertTemplateHeader('正文\n', '  \n', ctx);
  assert.equal(result.status, 'empty');
  assert.equal(result.content, '正文\n');
});

test('占位符替换 title 与 date', () => {
  const result = insertTemplateHeader('', '> {{title}} · {{date}}', ctx);
  assert.equal(result.status, 'inserted');
  assert.equal(result.content, '> 测试文档 · 2026-11-05\n');
});

test('front matter 与正文定位辅助函数', () => {
  const lines = ['<!-- @synapse-id: x -->', '---', 'pinned: true', '---', '', '正文'];
  assert.deepEqual(findFrontMatter(lines), { startLine: 1, endLine: 3 });
  assert.equal(bodyStartLine(lines), 5);
  assert.equal(isFrontMatterTemplate('---\npinned: true\n---'), true);
  assert.equal(isFrontMatterTemplate('> 字头'), false);
});

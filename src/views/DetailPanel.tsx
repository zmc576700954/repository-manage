import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import { RelationDialog } from '@/components/RelationDialog';
import type { Entry, Relation } from '@shared/types';

interface ExtLink {
  text: string;
  url: string;
  external: boolean;
}

function lookupTitle(entries: Entry[], id: string): string | null {
  return entries.find((e) => e.id === id)?.title ?? null;
}

// 简易客户端 markdown 链接解析，复用后端语义
function parseMarkdownLinksClient(content: string): ExtLink[] {
  const out: ExtLink[] = [];
  const re = /\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(content))) {
    const text = m[1].trim();
    const url = m[2].trim();
    if (!text || !url) continue;
    const before = content.slice(Math.max(0, m.index - 1), m.index);
    if (before === '!') continue;
    const external = /^(https?:\/\/|ftp:\/\/|mailto:|\/\/)/i.test(url);
    out.push({ text, url, external });
  }
  return out;
}

// 计算相对路径：基于 fromDir 解析 relUrl（处理 ./ ../ 等）
function normalizeRelPath(fromDir: string, relUrl: string): string {
  const parts = fromDir.split('/').filter(Boolean);
  for (const seg of relUrl.split('/')) {
    if (seg === '' || seg === '.') continue;
    if (seg === '..') {
      parts.pop();
    } else {
      parts.push(seg);
    }
  }
  return parts.join('/');
}

// 与 Rust 端 resolve_target 语义保持一致
function resolveInternalLink(
  linkUrl: string,
  fromEntry: Entry,
  entries: Entry[],
  kbRoot: string
): Entry | null {
  if (!linkUrl) return null;
  const cleaned = linkUrl.split('#')[0].split('?')[0];
  if (!cleaned) return null;
  const withoutExt = cleaned.replace(/\.md$/, '');

  // 1) 完整 id 匹配
  const byId = entries.find((e) => e.id === cleaned || e.id === withoutExt);
  if (byId) return byId;

  // 2) 基于 fromEntry 路径 normalize 后得到 kb-root 相对的 path
  if (kbRoot && fromEntry.path.startsWith(kbRoot)) {
    const fromRel = fromEntry.path
      .slice(kbRoot.length + 1)
      .replace(/\\/g, '/');
    const fromDir = fromRel.includes('/') ? fromRel.split('/').slice(0, -1).join('/') : '';
    const relResolved = normalizeRelPath(fromDir, cleaned);
    const candidate = entries.find((e) => {
      if (!e.path.startsWith(kbRoot)) return false;
      const eRel = e.path.slice(kbRoot.length + 1).replace(/\\/g, '/').replace(/\.md$/, '');
      return eRel === relResolved || eRel === withoutExt;
    });
    if (candidate) return candidate;
  }

  // 3) file stem 匹配
  const stem = withoutExt.split('/').pop() ?? withoutExt;
  const byStem = entries.find((e) => {
    const eStem = e.path.split('/').pop()?.replace(/\.md$/, '') ?? '';
    return eStem === stem;
  });
  if (byStem) return byStem;

  // 4) id 末尾匹配（target 是 entry id 的最后一段）
  return entries.find((e) => e.id === stem || e.id.endsWith(`-${stem}`)) ?? null;
}

export function DetailPanel() {
  const { t } = useTranslation('DetailPanel');
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);
  const entries = useAppStore((s) => s.entries);
  const kbs = useAppStore((s) => s.kbs);
  const activeKbId = useAppStore((s) => s.activeKbId);
  const selectEntry = useAppStore((s) => s.selectEntry);
  const deleteRelation = useAppStore((s) => s.deleteRelation);
  const addInternalReference = useAppStore((s) => s.createRelation);
  const createEntryFromReference = useAppStore((s) => s.createEntryFromReference);

  const relLabel = (type: string): string => {
    const known = ['reference', 'derived', 'contradicts', 'supersedes', 'extends', 'custom'];
    if (known.includes(type)) return t(`relLabel.${type}`);
    return t('relLabelFallback', { type });
  };
  const relDesc = (type: string, direction: 'outgoing' | 'incoming'): string => {
    const label = relLabel(type);
    return direction === 'outgoing'
      ? t('relDescriptionOutgoing', { label })
      : t('relDescriptionIncoming', { label });
  };

  const [content, setContent] = useState<{ markdown: string | null; error: string | null }>({
    markdown: null,
    error: null,
  });
  const [extLinks, setExtLinks] = useState<ExtLink[]>([]);
  const [showRelDialog, setShowRelDialog] = useState(false);
  const [linkBusy, setLinkBusy] = useState<number | null>(null);
  const [linkMsg, setLinkMsg] = useState<Record<number, string>>({});
  const [createDialog, setCreateDialog] = useState<{
    open: boolean;
    linkIndex: number;
    refText: string;
    sourceUrl: string;
  } | null>(null);

  const activeKb = useMemo(
    () => kbs.find((k) => k.id === activeKbId) ?? null,
    [kbs, activeKbId]
  );
  const kbRoot = activeKb?.path ?? '';
  const existingGroups = useMemo(() => {
    const set = new Set<string>();
    for (const e of entries) if (e.group) set.add(e.group);
    return Array.from(set).sort();
  }, [entries]);

  const entry = useMemo(
    () => entries.find((e) => e.id === selectedEntryId),
    [entries, selectedEntryId]
  );

  useEffect(() => {
    if (!selectedEntryId) {
      setContent({ markdown: null, error: null });
      setExtLinks([]);
      return;
    }
    let cancelled = false;
    setContent({ markdown: null, error: null });
    setExtLinks([]);
    setLinkMsg({});
    invoke<string | null>('get_entry_content', { id: selectedEntryId })
      .then((text) => {
        if (cancelled) return;
        if (text == null) {
          setContent({ markdown: null, error: '源文件不存在或无法读取' });
        } else {
          setContent({ markdown: text, error: null });
          setExtLinks(parseMarkdownLinksClient(text));
        }
      })
      .catch((e) => {
        if (cancelled) return;
        setContent({ markdown: null, error: (e as Error).message });
      });
    return () => {
      cancelled = true;
    };
  }, [selectedEntryId]);

  if (!entry) {
    return null;
  }

  const groupedRelations = useMemo(() => {
    const outgoing: Relation[] = [];
    const incoming: Relation[] = [];
    for (const rel of entry.relations) {
      if (rel.fromId === entry.id) outgoing.push(rel);
      else if (rel.toId === entry.id) incoming.push(rel);
    }
    return { outgoing, incoming };
  }, [entry]);

  const handleInternalLink = async (idx: number, link: ExtLink) => {
    if (!entry) return;
    const target = resolveInternalLink(link.url, entry, entries, kbRoot);
    if (!target) {
      // 解析不到对应 entry — 退化为创建新文档
      setCreateDialog({
        open: true,
        linkIndex: idx,
        refText: link.text,
        sourceUrl: link.url,
      });
      return;
    }
    setLinkBusy(idx);
    try {
      await addInternalReference(entry.id, target.id, 'reference', link.text);
      setLinkMsg((m) => ({ ...m, [idx]: t('linkMsgLinked', { title: target.title ?? target.id }) }));
    } catch (e) {
      setLinkMsg((m) => ({ ...m, [idx]: t('linkMsgFailed', { msg: (e as Error).message }) }));
    } finally {
      setLinkBusy(null);
    }
  };

  const handleCreateEntry = async (
    parentGroup: string,
    name: string
  ) => {
    if (!createDialog || !entry) return;
    setLinkBusy(createDialog.linkIndex);
    try {
      const newId = await createEntryFromReference(
        entry.id,
        parentGroup,
        name,
        createDialog.sourceUrl,
        createDialog.refText
      );
      if (newId) {
        setLinkMsg((m) => ({
          ...m,
          [createDialog.linkIndex]: t('linkMsgCreated', { id: newId }),
        }));
        setCreateDialog(null);
      } else {
        setLinkMsg((m) => ({
          ...m,
          [createDialog.linkIndex]: t('linkMsgCreateFailed'),
        }));
      }
    } catch (e) {
      setLinkMsg((m) => ({
        ...m,
        [createDialog.linkIndex]: t('linkMsgFailed', { msg: (e as Error).message }),
      }));
    } finally {
      setLinkBusy(null);
    }
  };

  return (
    <>
      <aside className="flex h-full w-[28rem] max-w-[40vw] flex-col border-l border-gray-200 bg-white">
        {/* Header */}
        <div className="flex items-start gap-3 border-b border-gray-200 p-4">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              {entry.group && (
                <span className="rounded bg-gray-100 px-2 py-0.5 text-xs font-medium text-gray-600">
                  {entry.group}
                </span>
              )}
              <span className="text-xs text-gray-400">{entry.id}</span>
            </div>
            <h2 className="mt-1 truncate text-lg font-semibold text-gray-900">
              {entry.title}
            </h2>
            <p className="mt-1 text-xs text-gray-500">
              {content.error
                ? content.error
                : content.markdown == null
                  ? t('loadingContent')
                  : t('contentIdle')}
            </p>
          </div>
          <button
            onClick={() => selectEntry(null)}
            className="shrink-0 rounded-md px-2 py-1 text-sm text-gray-500 hover:bg-gray-100"
            aria-label={t('closeAria')}
          >
            ✕
          </button>
        </div>

        {/* Scrollable content area */}
        <div className="flex-1 overflow-y-auto">
          {/* Tags */}
          {entry.tags.length > 0 && (
            <div className="border-b border-gray-200 p-4">
              <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
                {t('tagsHeader')}
              </h3>
              <div className="flex flex-wrap gap-1">
                {entry.tags.map((tag) => (
                  <span
                    key={tag}
                    className="rounded-full bg-blue-50 px-2 py-0.5 text-xs text-blue-700"
                  >
                    #{tag}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* Markdown content */}
          <div className="border-b border-gray-200 p-4">
            <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
              {t('contentHeader')}
            </h3>
            {content.markdown != null ? (
              <pre className="max-h-[60vh] overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800">
                {content.markdown}
              </pre>
            ) : (
              <p className="text-sm text-gray-400">{content.error ?? t('loadingContent')}</p>
            )}
          </div>

          {/* Outgoing relations */}
          <div className="border-b border-gray-200 p-4">
            <div className="mb-2 flex items-center justify-between">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-gray-500">
                {t('outgoingHeader', { count: groupedRelations.outgoing.length })}
              </h3>
              <button
                onClick={() => setShowRelDialog(true)}
                className="rounded-md bg-blue-500 px-2 py-1 text-xs font-medium text-white hover:bg-blue-600"
                title={t('addRelationTitle')}
              >
                {t('addRelation')}
              </button>
            </div>
            {groupedRelations.outgoing.length === 0 ? (
              <p className="text-sm text-gray-400">
                {t('outgoingEmpty', { action: t('addRelation') })}
              </p>
            ) : (
              <ul className="space-y-1">
                {groupedRelations.outgoing.map((rel, idx) => (
                  <li
                    key={`out-${idx}`}
                    className="flex items-start justify-between gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-gray-50"
                  >
                    <div className="min-w-0 flex-1">
                      <button
                        onClick={() => selectEntry(rel.toId)}
                        className="block w-full truncate text-left text-blue-700 hover:underline"
                        title={lookupTitle(entries, rel.toId) ?? rel.toId}
                      >
                        → {lookupTitle(entries, rel.toId) ?? rel.toId}
                      </button>
                      <p className="mt-0.5 text-xs text-gray-500">
                        {relDesc(rel.type, 'outgoing')}
                      </p>
                      {rel.note && (
                        <p className="mt-0.5 text-xs italic text-gray-500">"{rel.note}"</p>
                      )}
                    </div>
                    <button
                      onClick={() => deleteRelation(rel.fromId, rel.toId, rel.type)}
                      className="shrink-0 rounded px-1.5 py-0.5 text-xs text-red-600 hover:bg-red-50"
                      title={t('deleteRelTitle')}
                    >
                      ✕
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>

          {/* Incoming relations */}
          <div className="border-b border-gray-200 p-4">
            <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
              {t('incomingHeader', { count: groupedRelations.incoming.length })}
            </h3>
            {groupedRelations.incoming.length === 0 ? (
              <p className="text-sm text-gray-400">{t('incomingEmpty')}</p>
            ) : (
              <ul className="space-y-1">
                {groupedRelations.incoming.map((rel, idx) => (
                  <li
                    key={`in-${idx}`}
                    className="rounded-md px-2 py-1.5 text-sm hover:bg-gray-50"
                  >
                    <button
                      onClick={() => selectEntry(rel.fromId)}
                      className="block w-full truncate text-left text-blue-700 hover:underline"
                      title={lookupTitle(entries, rel.fromId) ?? rel.fromId}
                    >
                      ← {lookupTitle(entries, rel.fromId) ?? rel.fromId}
                    </button>
                    <p className="mt-0.5 text-xs text-gray-500">
                      {relDesc(rel.type, 'incoming')}
                    </p>
                    {rel.note && (
                      <p className="mt-0.5 text-xs italic text-gray-500">"{rel.note}"</p>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>

          {/* External references (URLs) */}
          {extLinks.length > 0 && (
            <div className="border-b border-gray-200 p-4">
              <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
                {t('referencesHeader', { count: extLinks.length })}
              </h3>
              <p className="mb-2 text-xs text-gray-500">
                {t('referencesHint')}
              </p>
              <ul className="space-y-1.5">
                {extLinks.map((link, idx) => {
                  const resolved = !link.external
                    ? resolveInternalLink(link.url, entry, entries, kbRoot)
                    : null;
                  const msg = linkMsg[idx];
                  const busy = linkBusy === idx;
                  return (
                    <li
                      key={idx}
                      className="flex flex-col gap-1 rounded-md bg-gray-50 px-2 py-1.5 text-sm"
                    >
                      <div className="flex items-start gap-2">
                        <span className="text-gray-400">
                          {link.external ? '🔗' : '📄'}
                        </span>
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-gray-800" title={link.text}>
                            {link.text}
                          </div>
                          <div
                            className="truncate font-mono text-xs text-gray-500"
                            title={link.url}
                          >
                            {link.url}
                          </div>
                          {resolved && (
                            <div className="mt-0.5 text-xs text-green-700">
                              {t('matchedTarget', { title: resolved.title ?? resolved.id })}
                            </div>
                          )}
                          {msg && (
                            <div className="mt-0.5 text-xs text-blue-700">{msg}</div>
                          )}
                        </div>
                        {link.external ? (
                          <button
                            onClick={() => {
                              navigator.clipboard.writeText(link.url).catch(() => {});
                            }}
                            className="shrink-0 rounded border border-gray-300 bg-white px-2 py-0.5 text-xs text-gray-600 hover:bg-gray-100"
                            title={t('copyTitle')}
                          >
                            {t('copyButton')}
                          </button>
                        ) : null}
                      </div>
                      <div className="flex flex-wrap items-center gap-2 pl-5">
                        {!link.external && (
                          <button
                            disabled={busy}
                            onClick={() => handleInternalLink(idx, link)}
                            className="rounded-md bg-blue-500 px-2 py-0.5 text-xs font-medium text-white hover:bg-blue-600 disabled:opacity-50"
                            title={
                              resolved
                                ? t('internalLinkHint', { title: resolved.title ?? resolved.id })
                                : t('internalUnresolvedHint')
                            }
                          >
                            {busy ? t('internalBusy') : resolved ? t('internalLink') : t('internalCreate')}
                          </button>
                        )}
                        <button
                          disabled={busy}
                          onClick={() =>
                            setCreateDialog({
                              open: true,
                              linkIndex: idx,
                              refText: link.text,
                              sourceUrl: link.url,
                            })
                          }
                          className="rounded-md border border-gray-300 bg-white px-2 py-0.5 text-xs text-gray-700 hover:bg-gray-100 disabled:opacity-50"
                          title={t('createFromRefTitle')}
                        >
                          {t('createFromRefButton')}
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            </div>
          )}

          {/* Metadata footer */}
          <div className="p-4 text-xs text-gray-400">
            <div>
              <span className="font-semibold text-gray-600">{t('sourceFile')}</span>{' '}
              <span className="break-all font-mono">{entry.path}</span>
            </div>
            <button
              onClick={() => selectEntry(null)}
              className="mt-3 text-blue-600 hover:underline"
            >
              {t('closeFooter')}
            </button>
          </div>
        </div>
      </aside>
      {showRelDialog && (
        <RelationDialog fromId={entry.id} onClose={() => setShowRelDialog(false)} />
      )}
      {createDialog && (
        <CreateEntryDialog
          refText={createDialog.refText}
          sourceUrl={createDialog.sourceUrl}
          existingGroups={existingGroups}
          busy={linkBusy === createDialog.linkIndex}
          onCancel={() => setCreateDialog(null)}
          onSubmit={handleCreateEntry}
        />
      )}
    </>
  );
}

interface CreateEntryDialogProps {
  refText: string;
  sourceUrl: string;
  existingGroups: string[];
  busy: boolean;
  onCancel: () => void;
  onSubmit: (parentGroup: string, name: string) => void;
}

function CreateEntryDialog({
  refText,
  sourceUrl,
  existingGroups,
  busy,
  onCancel,
  onSubmit,
}: CreateEntryDialogProps) {
  const { t } = useTranslation('DetailPanel');
  const [parentGroup, setParentGroup] = useState(existingGroups[0] ?? '');
  const [name, setName] = useState(() => slugFromText(refText));

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">{t('createRelHeader')}</h2>
        <p className="mt-1 text-sm text-gray-500">
          {t('createRelHint')}
        </p>
        <div className="mt-3 rounded-md bg-gray-50 p-2 text-xs text-gray-600">
          <div>
            <span className="font-semibold">{t('createRelRefText')}</span>
            {refText}
          </div>
          <div className="mt-1 break-all font-mono text-[11px] text-gray-500">
            {sourceUrl}
          </div>
        </div>

        <div className="mt-4 space-y-3">
          <div>
            <label className="block text-sm font-medium text-gray-700">
              {t('createRelGroupLabel')}
            </label>
            <input
              list="kb-groups"
              value={parentGroup}
              onChange={(e) => setParentGroup(e.target.value.trim())}
              placeholder={t('createRelGroupPlaceholder')}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            />
            <datalist id="kb-groups">
              {existingGroups.map((g) => (
                <option key={g} value={g} />
              ))}
            </datalist>
            <p className="mt-1 text-xs text-gray-500">
              {t('createRelGroupHelp')}
            </p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">
              {t('createRelNameLabel')}
            </label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value.trim())}
              placeholder={t('createRelNamePlaceholder')}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm font-mono"
            />
            <p className="mt-1 text-xs text-gray-500">
              {t('createRelSavePathHint')} <span className="font-mono">{'<kb-root>/' + (parentGroup || '<group>') + '/' + (name || '<name>') + '.md'}</span>
            </p>
          </div>
        </div>

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
          >
            {t('createRelCancel')}
          </button>
          <button
            disabled={busy || !parentGroup || !name}
            onClick={() => onSubmit(parentGroup, name)}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {busy ? t('createRelSubmitBusy') : t('createRelSubmitIdle')}
          </button>
        </div>
      </div>
    </div>
  );
}

function slugFromText(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9一-龥]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 40);
}

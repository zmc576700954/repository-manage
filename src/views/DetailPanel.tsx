import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import { usePanelStore } from '@/store/panelStore';
import { RelationDialog } from '@/components/RelationDialog';
import { EntryMetadataEditor } from '@/components/EntryMetadataEditor';
import { RenameEntryDialog } from '@/components/RenameEntryDialog';
import { VersionHistorySection } from '@/components/VersionHistorySection';
import { MarkdownView } from '@/markdown/MarkdownView';
import { localizeError } from '@/i18n/translateError';
import { clearDraft, pendingDraft, writeDraft, type EntryDraft } from '@/utils/editorDraft';
import { changedLinePairs } from '@/utils/lineDiff';
import { insertTemplateHeader, type TemplateInsertResult } from '@/utils/templateHeader';
import type { Entry, FormatPreview, Relation } from '@shared/types';

interface ExtLink {
  text: string;
  url: string;
  external: boolean;
}

const DRAFT_AUTOSAVE_MS = 30_000;

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
  const { t: tMd } = useTranslation('MarkdownView');
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);
  const entriesRevision = useAppStore((s) => s.entriesRevision);
  const entries = useAppStore((s) => s.entries);
  const kbs = useAppStore((s) => s.kbs);
  const activeKbId = useAppStore((s) => s.activeKbId);
  const selectEntry = useAppStore((s) => s.selectEntry);
  const deleteRelation = useAppStore((s) => s.deleteRelation);
  const saveEntryContent = useAppStore((s) => s.saveEntryContent);
  const autosaveEntryContent = useAppStore((s) => s.autosaveEntryContent);
  const previewFormatEntry = useAppStore((s) => s.previewFormatEntry);
  const applyFormatEntry = useAppStore((s) => s.applyFormatEntry);
  const addInternalReference = useAppStore((s) => s.createRelation);
  const createEntryFromReference = useAppStore((s) => s.createEntryFromReference);
  const templateHeader = useAppStore((s) => s.templateHeader);
  const closePanel = usePanelStore((s) => s.close);
  const openPanel = usePanelStore((s) => s.open);

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
  /// 已加载的条目 id 与正文原文：列表刷新但正文没变时保持当前渲染，
  /// 不重置滚动位置、不替换用户正在查看的源码视图。
  /// 直接比较原文（而非哈希）：零碰撞、无需额外计算，代价只是多持有
  /// 一份已显示过的字符串引用。
  const loadedEntryIdRef = useRef<string | null>(null);
  const lastContentRef = useRef<string | null>(null);
  const [viewMode, setViewMode] = useState<'rendered' | 'source' | 'edit'>('rendered');
  const [extLinks, setExtLinks] = useState<ExtLink[]>([]);

  // ---------- 正文编辑器 ----------
  /// 编辑器里的文本与 dirty 状态放在 ref 里同步维护：自动保存 interval、
/// 切条目时的草稿落盘、外部变更判定都在异步回调里读它们，不能依赖
/// 可能过期的闭包状态。
  const [editorText, setEditorText] = useState('');
  const editorTextRef = useRef('');
  const [dirty, setDirty] = useState(false);
  const dirtyRef = useRef(false);
  const viewModeRef = useRef(viewMode);
  const draftOwnerRef = useRef<{ kbId: string | null; entryId: string } | null>(null);
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle');
  const [saveError, setSaveError] = useState<string | null>(null);
  const [draftSavedAt, setDraftSavedAt] = useState<number | null>(null);
  const [pendingDraftState, setPendingDraftState] = useState<EntryDraft | null>(null);
  const [externalText, setExternalText] = useState<string | null>(null);
  const [externalChanged, setExternalChanged] = useState(false);
  const [formatPreview, setFormatPreview] = useState<FormatPreview | null>(null);
  const [formatBusy, setFormatBusy] = useState(false);
  const [formatNotice, setFormatNotice] = useState<string | null>(null);
  const [headerBusy, setHeaderBusy] = useState(false);
  const [headerNotice, setHeaderNotice] = useState<string | null>(null);
  const [headerWarn, setHeaderWarn] = useState(false);
  const [showMove, setShowMove] = useState(false);

  useEffect(() => {
    viewModeRef.current = viewMode;
  }, [viewMode]);

  const [showRelDialog, setShowRelDialog] = useState(false);
  const [showRename, setShowRename] = useState(false);
  const setInteractionBusy = useAppStore((s) => s.setInteractionBusy);
  const renamingEntryId = useAppStore((s) => s.renamingEntryId);
  const setRenamingEntryId = useAppStore((s) => s.setRenamingEntryId);
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
  // 新建/重命名用的候选目录必须是物理路径 `dir`；`group` 只做展示，
  // 用它拼路径会把嵌套条目压平。
  const existingGroups = useMemo(() => {
    const set = new Set<string>();
    for (const e of entries) if (e.dir) set.add(e.dir);
    return Array.from(set).sort();
  }, [entries]);

  const entry = useMemo(
    () => entries.find((e) => e.id === selectedEntryId),
    [entries, selectedEntryId]
  );

  // 正文加载：选中项变化时重置并拉取；列表因 watcher 事件刷新
  // （entriesRevision 变化）而选中项不变时，重新拉取正文并与原文比对，
  // 只有正文真的变了才替换内容。这样外部直接编辑 `.md` 正文能自动反映，
  // 同时不会覆盖用户正在输入的正文/元数据，也不抢焦点（内容未变时完全
  // 不 setState）。直接比较原文比哈希更可靠，也不额外消耗计算。
  useEffect(() => {
    const resetEditor = () => {
      setEditorText('');
      editorTextRef.current = '';
      setDirty(false);
      dirtyRef.current = false;
      setSaveState('idle');
      setSaveError(null);
      setDraftSavedAt(null);
      setPendingDraftState(null);
      setExternalText(null);
      setExternalChanged(false);
      setFormatPreview(null);
      setFormatNotice(null);
      setHeaderNotice(null);
      setHeaderWarn(false);
      setHeaderBusy(false);
      setShowMove(false);
    };

    // 切换条目/关闭面板时，先把上一条目未保存的草稿落到 localStorage。
    const prevOwner = draftOwnerRef.current;
    if (prevOwner && dirtyRef.current) {
      writeDraft(prevOwner.kbId, prevOwner.entryId, editorTextRef.current);
    }
    draftOwnerRef.current = selectedEntryId
      ? { kbId: activeKbId, entryId: selectedEntryId }
      : null;

    if (!selectedEntryId) {
      loadedEntryIdRef.current = null;
      lastContentRef.current = null;
      setContent({ markdown: null, error: null });
      setExtLinks([]);
      setViewMode('rendered');
      resetEditor();
      return;
    }
    const sameEntry = loadedEntryIdRef.current === selectedEntryId;
    loadedEntryIdRef.current = selectedEntryId;
    let cancelled = false;
    if (!sameEntry) {
      setContent({ markdown: null, error: null });
      setExtLinks([]);
      setLinkMsg({});
      setViewMode('rendered');
      lastContentRef.current = null;
      resetEditor();
    }
    invoke<string | null>('get_entry_content', { id: selectedEntryId })
      .then((text) => {
        if (cancelled) return;
        if (sameEntry && text === lastContentRef.current) {
          // 正文没变：保持当前内容（含滚动位置与渲染/源码视图）。
          return;
        }
        // 用户正在编辑且有未保存修改：不能直接替换编辑器内容，只提示
        // 外部变更，由用户决定重新加载还是保留自己的编辑。
        if (viewModeRef.current === 'edit' && dirtyRef.current) {
          setExternalText(text);
          setExternalChanged(true);
          return;
        }
        lastContentRef.current = text;
        if (text == null) {
          setContent({ markdown: null, error: t('contentErrorMissing') });
          setExtLinks([]);
        } else {
          setContent({ markdown: text, error: null });
          setExtLinks(parseMarkdownLinksClient(text));
          if (!sameEntry) {
            // 只在首次进入该条目时询问草稿，避免列表刷新反复弹提示。
            setPendingDraftState(pendingDraft(activeKbId, selectedEntryId, text));
          }
        }
      })
      .catch((e) => {
        if (cancelled) return;
        lastContentRef.current = null;
        setContent({ markdown: null, error: (e as Error).message });
      });
    return () => {
      cancelled = true;
    };
  }, [selectedEntryId, entriesRevision, t, activeKbId]);

  // 自动保存：编辑模式下每 30 秒写盘一次。
  // - 先在 localStorage 存一份草稿（崩溃兜底，不触发 watcher）；
  // - 再调用 autosave 命令真正落盘，版本记录由后端按“≥5 分钟”节流。
  // - 保存期间用户继续输入时，只更新 lastContentRef，不覆盖编辑器内容。
  useEffect(() => {
    if (!selectedEntryId || viewMode !== 'edit') return;
    let cancelled = false;
    const timer = setInterval(() => {
      if (!dirtyRef.current) return;
      const text = editorTextRef.current;
      writeDraft(activeKbId, selectedEntryId, text);
      setDraftSavedAt(Date.now());
      void autosaveEntryContent(selectedEntryId, text)
        .then((result) => {
          if (cancelled) return;
          lastContentRef.current = result.content;
          if (editorTextRef.current === text) {
            setContent({ markdown: result.content, error: null });
            setExtLinks(parseMarkdownLinksClient(result.content));
            setDirty(false);
            dirtyRef.current = false;
            clearDraft(activeKbId, selectedEntryId);
            setDraftSavedAt(null);
            setSaveState('saved');
          }
        })
        .catch(() => {
          // 落盘失败时保留 dirty 与本地草稿，下一次显式保存会重试。
        });
    }, DRAFT_AUTOSAVE_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
      if (dirtyRef.current) {
        writeDraft(activeKbId, selectedEntryId, editorTextRef.current);
        setDraftSavedAt(Date.now());
      }
    };
  }, [selectedEntryId, activeKbId, viewMode, autosaveEntryContent]);

  /// Cycle 3 polish B: when the store asks us to open the rename dialog
  /// for an id, open it. We clear the store value back to null so the
  /// dialog doesn't reopen if the user closes + reopens it.
  useEffect(() => {
    if (renamingEntryId !== null && renamingEntryId === selectedEntryId) {
      setShowRename(true);
      setRenamingEntryId(null);
    } else if (renamingEntryId !== null && renamingEntryId !== selectedEntryId) {
      // Different entry requested: select it first so DetailPanel
      // re-renders with the new entry, then the dialog will open.
      selectEntry(renamingEntryId);
      openPanel('detail');
    }
  }, [renamingEntryId, selectedEntryId, selectEntry, openPanel, setRenamingEntryId]);

  // 重命名对话框打开期间标记交互忙：watcher 的自动刷新会推迟到关闭之后，
  // 避免外部变更在对话框开着时把列表/面板刷掉或抢走焦点。
  useEffect(() => {
    setInteractionBusy(showRename);
    return () => setInteractionBusy(false);
  }, [showRename, setInteractionBusy]);

  // 所有 Hook 必须无条件执行：重命名过程中 selectedEntryId 会切到新 id，
  // 期间 entries 还没刷新完，entry 可能短暂为 undefined。如果把
  // `if (!entry) return null` 放在 hook 之前，这次渲染会少调用一个 Hook，
  // React 直接报 "Rendered fewer hooks than expected" 并让整个 UI 崩溃。
  const groupedRelations = useMemo(() => {
    const outgoing: Relation[] = [];
    const incoming: Relation[] = [];
    if (!entry) return { outgoing, incoming };
    for (const rel of entry.relations) {
      if (rel.fromId === entry.id) outgoing.push(rel);
      else if (rel.toId === entry.id) incoming.push(rel);
    }
    return { outgoing, incoming };
  }, [entry]);

  if (!entry) {
    return null;
  }

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

  const handleWikilinkNavigate = (targetId: string) => {
    selectEntry(targetId);
  };

  const handleWikilinkCreate = (targetId: string, caption: string) => {
    if (!entry) return;
    setCreateDialog({
      open: true,
      linkIndex: -1,
      refText: caption || targetId,
      sourceUrl: targetId,
    });
  };

  const resolveWikilinkTarget = (target: string) => {
    if (!entry) return null;
    const found = resolveInternalLink(target, entry, entries, kbRoot);
    return found ? { id: found.id, title: found.title } : null;
  };

  // ---------- 编辑器动作 ----------

  const handleEnterEdit = () => {
    if (content.markdown == null) return;
    setEditorText(content.markdown);
    editorTextRef.current = content.markdown;
    setDirty(false);
    dirtyRef.current = false;
    setSaveState('idle');
    setSaveError(null);
    setDraftSavedAt(null);
    setViewMode('edit');
  };

  const handleEditorChange = (value: string) => {
    setEditorText(value);
    editorTextRef.current = value;
    if (!dirtyRef.current) {
      dirtyRef.current = true;
      setDirty(true);
    }
    if (saveState !== 'saving') setSaveState('idle');
    setSaveError(null);
  };

  const applySavedContent = (text: string) => {
    lastContentRef.current = text;
    setContent({ markdown: text, error: null });
    setExtLinks(parseMarkdownLinksClient(text));
    setEditorText(text);
    editorTextRef.current = text;
    setDirty(false);
    dirtyRef.current = false;
    setDraftSavedAt(null);
    setSaveState('saved');
  };

  const handleSave = async (): Promise<boolean> => {
    if (!entry) return false;
    setSaveState('saving');
    setSaveError(null);
    try {
      const result = await saveEntryContent(entry.id, editorTextRef.current);
      applySavedContent(result.content);
      clearDraft(activeKbId, entry.id);
      setPendingDraftState(null);
      setExternalChanged(false);
      setExternalText(null);
      return true;
    } catch (e) {
      setSaveState('error');
      setSaveError(t('saveError', { msg: localizeError(e) }));
      return false;
    }
  };

  // ---------- 标准模版字头 ----------
  const todayIso = (): string => {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  };

  const describeHeaderResult = (result: TemplateInsertResult): string | null => {
    switch (result.status) {
      case 'empty':
        return t('headerNotConfigured');
      case 'conflict':
        return t('headerFrontMatterConflict');
      case 'already-present':
        return t('headerAlreadyPresent');
      default:
        return null;
    }
  };

  const handleInsertHeader = async () => {
    if (!entry) return;
    setHeaderNotice(null);
    setHeaderWarn(false);

    // 编辑态：插入编辑器文本，由用户按保存按钮写盘。
    if (viewModeRef.current === 'edit') {
      const result = insertTemplateHeader(editorTextRef.current, templateHeader, {
        title: entry.title,
        date: todayIso(),
      });
      const problem = describeHeaderResult(result);
      if (problem) {
        setHeaderNotice(problem);
        setHeaderWarn(true);
        return;
      }
      setEditorText(result.content);
      editorTextRef.current = result.content;
      dirtyRef.current = true;
      setDirty(true);
      setSaveState('idle');
      setSaveError(null);
      setHeaderNotice(t('headerInsertedUnsaved'));
      return;
    }

    if (content.markdown == null) return;
    const base = content.markdown;
    const result = insertTemplateHeader(base, templateHeader, {
      title: entry.title,
      date: todayIso(),
    });
    const problem = describeHeaderResult(result);
    if (problem) {
      setHeaderNotice(problem);
      setHeaderWarn(true);
      return;
    }

    setHeaderBusy(true);
    try {
      const saved = await saveEntryContent(entry.id, result.content);
      applySavedContent(saved.content);
      clearDraft(activeKbId, entry.id);
      setPendingDraftState(null);
      setExternalChanged(false);
      setExternalText(null);
      setHeaderNotice(t('headerInserted'));
    } catch (e) {
      setHeaderNotice(t('headerInsertError', { msg: localizeError(e) }));
      setHeaderWarn(true);
    } finally {
      setHeaderBusy(false);
    }
  };

  const handlePreviewFormat = async () => {
    if (!entry) return;
    setFormatBusy(true);
    setFormatNotice(null);
    try {
      const preview = await previewFormatEntry(entry.id);
      if (!preview.changed) {
        setFormatNotice(t('formatNoChanges'));
      } else {
        setFormatPreview(preview);
      }
    } catch (e) {
      setFormatNotice(t('formatError', { msg: localizeError(e) }));
    } finally {
      setFormatBusy(false);
    }
  };

  const handleApplyFormat = async () => {
    if (!entry || !formatPreview) return;
    setFormatBusy(true);
    try {
      const result = await applyFormatEntry(entry.id);
      setFormatPreview(null);
      applySavedContent(result.content);
      clearDraft(activeKbId, entry.id);
      setPendingDraftState(null);
      setFormatNotice(t('formatApplied'));
    } catch (e) {
      setFormatNotice(t('formatError', { msg: localizeError(e) }));
    } finally {
      setFormatBusy(false);
    }
  };

  const handleReloadExternal = () => {
    if (!entry || externalText == null) return;
    applySavedContent(externalText);
    setSaveState('idle');
    clearDraft(activeKbId, entry.id);
    setPendingDraftState(null);
    setExternalChanged(false);
    setExternalText(null);
  };

  const handleKeepEdits = () => {
    if (externalText != null) lastContentRef.current = externalText;
    setExternalChanged(false);
    setExternalText(null);
  };

  const handleRestoreDraft = () => {
    if (!pendingDraftState) return;
    setEditorText(pendingDraftState.content);
    editorTextRef.current = pendingDraftState.content;
    dirtyRef.current = true;
    setDirty(true);
    setSaveState('idle');
    setSaveError(null);
    setPendingDraftState(null);
    setViewMode('edit');
  };

  const handleDiscardDraft = () => {
    if (entry) clearDraft(activeKbId, entry.id);
    setPendingDraftState(null);
  };

  const formatChanges = formatPreview
    ? changedLinePairs(formatPreview.original, formatPreview.formatted)
    : [];

  return (
    <>
      <div className="flex h-full flex-col">
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
            onClick={() => setShowMove(true)}
            className="shrink-0 rounded-md border border-gray-300 bg-white px-2 py-1 text-xs text-gray-700 hover:bg-gray-100"
            title={t('moveTitle')}
          >
            {t('moveButton')}
          </button>
          <button
            onClick={() => setShowRename(true)}
            className="shrink-0 rounded-md border border-gray-300 bg-white px-2 py-1 text-xs text-gray-700 hover:bg-gray-100"
            title={t('renameTitle')}
          >
            ✎
          </button>
          <button
            onClick={() => {
              selectEntry(null);
              closePanel('detail');
            }}
            className="shrink-0 rounded-md px-2 py-1 text-sm text-gray-500 hover:bg-gray-100"
            aria-label={t('closeAria')}
          >
            ✕
          </button>
        </div>

        {/* Scrollable content area */}
        <div className="flex-1 overflow-y-auto">
          {/* Inline metadata editor (title/tags/labels/status) */}
          <EntryMetadataEditor entry={entry} />

          {/* frontmatter 旁路元数据只读展示（kaiwu 四键等）。
              旧快照/旧条目没有 extraMeta 字段时整段不渲染。 */}
          {entry.extraMeta && entry.extraMeta.length > 0 && (
            <div className="border-b border-gray-200 p-4">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-gray-500">
                {t('extraMetaHeader')}
              </h3>
              <dl className="mt-2 space-y-1 text-xs">
                {entry.extraMeta.map((m) => (
                  <div key={m.key} className="flex gap-2">
                    <dt className="shrink-0 font-medium text-gray-700">{m.key}</dt>
                    <dd className="min-w-0 break-words text-gray-600">
                      {typeof m.value === 'string' ? m.value : JSON.stringify(m.value)}
                    </dd>
                  </div>
                ))}
              </dl>
            </div>
          )}

          {/* Markdown content */}
          <div className="border-b border-gray-200 p-4">
            <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-gray-500">
                {t('contentHeader')}
              </h3>
              {content.markdown != null && (
                <div className="flex flex-wrap items-center gap-1">
                  <div className="flex gap-1 rounded-md border border-gray-200 bg-white p-0.5 text-[11px]">
                    <button
                      onClick={() => setViewMode('rendered')}
                      className={`rounded px-2 py-0.5 ${
                        viewMode === 'rendered'
                          ? 'bg-blue-100 text-blue-700'
                          : 'text-gray-500 hover:bg-gray-50'
                      }`}
                    >
                      {tMd('viewToggleRender')}
                    </button>
                    <button
                      onClick={() => setViewMode('source')}
                      className={`rounded px-2 py-0.5 ${
                        viewMode === 'source'
                          ? 'bg-blue-100 text-blue-700'
                          : 'text-gray-500 hover:bg-gray-50'
                      }`}
                    >
                      {tMd('viewToggleSource')}
                    </button>
                    <button
                      onClick={handleEnterEdit}
                      className={`rounded px-2 py-0.5 ${
                        viewMode === 'edit'
                          ? 'bg-blue-100 text-blue-700'
                          : 'text-gray-500 hover:bg-gray-50'
                      }`}
                    >
                      {t('viewEdit')}
                    </button>
                  </div>
                  <button
                    onClick={handlePreviewFormat}
                    disabled={formatBusy}
                    title={t('formatHint')}
                    className="rounded-md border border-gray-300 bg-white px-2 py-0.5 text-[11px] text-gray-700 hover:bg-gray-100 disabled:opacity-50"
                  >
                    {t('formatButton')}
                  </button>
                  <button
                    onClick={handleInsertHeader}
                    disabled={headerBusy}
                    title={t('headerHint')}
                    className="rounded-md border border-gray-300 bg-white px-2 py-0.5 text-[11px] text-gray-700 hover:bg-gray-100 disabled:opacity-50"
                  >
                    {headerBusy ? t('headerBusy') : t('headerButton')}
                  </button>
                  {viewMode === 'edit' && (
                    <button
                      onClick={handleSave}
                      disabled={saveState === 'saving'}
                      className="rounded-md bg-blue-500 px-2 py-0.5 text-[11px] font-medium text-white hover:bg-blue-600 disabled:opacity-50"
                    >
                      {saveState === 'saving' ? t('saveBusy') : t('saveButton')}
                    </button>
                  )}
                </div>
              )}
            </div>

            {viewMode === 'edit' && (
              <div className="mb-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px]">
                {saveState === 'error' && saveError ? (
                  <span className="text-red-600">{saveError}</span>
                ) : saveState === 'saved' ? (
                  <span className="text-green-700">{t('saveDone')}</span>
                ) : dirty ? (
                  <span className="text-amber-600">{t('saveIdle')}</span>
                ) : null}
                {draftSavedAt != null && (
                  <span className="text-gray-400">
                    {t('draftSaved', {
                      time: new Date(draftSavedAt).toLocaleTimeString(),
                    })}
                  </span>
                )}
                <span className="text-gray-400">{t('autoSaveHint')}</span>
              </div>
            )}

            {pendingDraftState && (
              <div className="mb-2 flex flex-wrap items-center gap-2 rounded-md border border-amber-200 bg-amber-50 px-2 py-1.5 text-xs text-amber-800">
                <span>{t('draftPending')}</span>
                <button
                  onClick={handleRestoreDraft}
                  className="rounded border border-amber-300 bg-white px-2 py-0.5 hover:bg-amber-100"
                >
                  {t('draftRestore')}
                </button>
                <button
                  onClick={handleDiscardDraft}
                  className="rounded px-2 py-0.5 text-amber-700 hover:bg-amber-100"
                >
                  {t('draftDiscard')}
                </button>
              </div>
            )}

            {externalChanged && (
              <div className="mb-2 flex flex-wrap items-center gap-2 rounded-md border border-red-200 bg-red-50 px-2 py-1.5 text-xs text-red-700">
                <span>{t('externalChanged')}</span>
                <button
                  onClick={handleReloadExternal}
                  className="rounded border border-red-300 bg-white px-2 py-0.5 hover:bg-red-100"
                >
                  {t('externalReload')}
                </button>
                <button
                  onClick={handleKeepEdits}
                  className="rounded px-2 py-0.5 hover:bg-red-100"
                >
                  {t('externalKeep')}
                </button>
              </div>
            )}

            {formatNotice && <p className="mb-2 text-xs text-blue-700">{formatNotice}</p>}
            {headerNotice && (
              <p className={`mb-2 text-xs ${headerWarn ? 'text-amber-700' : 'text-green-700'}`}>
                {headerNotice}
              </p>
            )}

            {content.markdown != null ? (
              viewMode === 'rendered' ? (
                <MarkdownView
                  source={content.markdown}
                  resolveTarget={resolveWikilinkTarget}
                  onNavigate={handleWikilinkNavigate}
                  onCreateFromRef={handleWikilinkCreate}
                />
              ) : viewMode === 'source' ? (
                <pre className="max-h-[60vh] overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800">
                  {content.markdown}
                </pre>
              ) : (
                <textarea
                  value={editorText}
                  onChange={(e) => handleEditorChange(e.target.value)}
                  onKeyDown={(e) => {
                    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
                      e.preventDefault();
                      void handleSave();
                    }
                  }}
                  placeholder={t('editPlaceholder')}
                  spellCheck={false}
                  className="h-[60vh] w-full resize-y rounded-md border border-gray-200 bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800 focus:border-blue-400 focus:outline-none"
                />
              )
            ) : (
              <p className="text-sm text-gray-400">{content.error ?? t('loadingContent')}</p>
            )}
          </div>

          {/* Version history */}
          <VersionHistorySection
            entryId={entry.id}
            refreshKey={entriesRevision}
            editingDirty={dirty}
            onRestored={(text) => {
              applySavedContent(text);
              clearDraft(activeKbId, entry.id);
              setPendingDraftState(null);
              setExternalChanged(false);
              setExternalText(null);
            }}
          />

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
              <ul className="list-enter space-y-1">
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
                        {relDesc(rel.relationType, 'outgoing')}
                      </p>
                      {rel.note && (
                        <p className="mt-0.5 text-xs italic text-gray-500">"{rel.note}"</p>
                      )}
                    </div>
                    <button
                      onClick={() => deleteRelation(rel.fromId, rel.toId, rel.relationType)}
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
              <ul className="list-enter space-y-1">
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
                      {relDesc(rel.relationType, 'incoming')}
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
              onClick={() => {
                selectEntry(null);
                closePanel('detail');
              }}
              className="mt-3 text-blue-600 hover:underline"
            >
              {t('closeFooter')}
            </button>
          </div>
        </div>
      </div>
      {showRelDialog && (
        <RelationDialog
          fromId={entry.id}
          onClose={() => setShowRelDialog(false)}
          beforeCreate={async () =>
            dirtyRef.current ? await handleSave() : true
          }
        />
      )}
      {showRename && (
        <RenameEntryDialog
          entryId={entry.id}
          initialName={entry.id.split('-').pop() ?? entry.id}
          initialGroup={entry.dir ?? ''}
          existingGroups={existingGroups}
          onClose={() => setShowRename(false)}
          onSuccess={(newId) => {
            setShowRename(false);
            selectEntry(newId);
          }}
        />
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
      {showMove && (
        <MoveEntryDialog
          entryId={entry.id}
          initialDir={entry.dir ?? ''}
          entryName={
            entry.contentPath !== entry.path
              ? entry.path.split('/').pop() || entry.id
              : (entry.path.split('/').pop() ?? entry.id).replace(/\.md$/, '')
          }
          isDirectory={entry.contentPath !== entry.path}
          attachmentCount={entry.attachments.length}
          existingGroups={existingGroups}
          onClose={() => setShowMove(false)}
          onSuccess={(newId) => {
            setShowMove(false);
            selectEntry(newId);
          }}
        />
      )}
      {formatPreview && (
        <FormatPreviewDialog
          changes={formatChanges}
          changedLines={formatPreview.changedLines}
          busy={formatBusy}
          onCancel={() => setFormatPreview(null)}
          onApply={handleApplyFormat}
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

interface MoveEntryDialogProps {
  entryId: string;
  initialDir: string;
  /// 条目自身的目录名/文件名（不含扩展名），用于展示迁移后的 id。
  entryName: string;
  /// 目录型条目（content.md）随整个目录迁移，附件一并移动。
  isDirectory: boolean;
  attachmentCount: number;
  existingGroups: string[];
  onClose: () => void;
  onSuccess: (newId: string) => void;
}

function normalizeDir(dir: string): string {
  return dir
    .trim()
    .replace(/\\/g, '/')
    .replace(/^\/+|\/+$/g, '');
}

/// 与后端 derive_id 规则一致：目录型条目 id 只取自身名字；扁平条目
/// 在非根目录时使用 `目录-文件名`（路径分隔符替换为 `-`）。
function moveTargetId(targetDir: string, entryName: string, isDirectory: boolean): string {
  if (isDirectory || !targetDir) return entryName;
  return `${targetDir}-${entryName}`.replace(/[\\/]/g, '-');
}

/// 同 KB 迁移对话框：目录层级选择 + 自定义目录，展示迁移后的路径与 id。
/// 后端会一致迁移 id、级联引用、布局坐标、附件与历史目录，失败整体回滚。
function MoveEntryDialog({
  entryId,
  initialDir,
  entryName,
  isDirectory,
  attachmentCount,
  existingGroups,
  onClose,
  onSuccess,
}: MoveEntryDialogProps) {
  const { t } = useTranslation('DetailPanel');
  const moveEntry = useAppStore((s) => s.moveEntry);
  const currentDir = normalizeDir(initialDir);
  const [targetDir, setTargetDir] = useState(currentDir);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 目录树：把已有 dir 拆成逐级路径，排序后按层级缩进展示。
  const directories = useMemo(() => {
    const set = new Set<string>();
    for (const group of existingGroups) {
      const parts = normalizeDir(group).split('/').filter(Boolean);
      for (let i = 1; i <= parts.length; i += 1) {
        set.add(parts.slice(0, i).join('/'));
      }
    }
    const selfPath = currentDir ? `${currentDir}/${entryName}` : entryName;
    return Array.from(set)
      .filter((dir) => {
        if (!isDirectory) return true;
        // 不能把目录移动进它自己内部。
        return !(dir === selfPath || dir.startsWith(`${selfPath}/`));
      })
      .sort();
  }, [existingGroups, currentDir, entryName, isDirectory]);

  const normalizedTarget = normalizeDir(targetDir);
  const unchanged = normalizedTarget === currentDir;
  const newId = moveTargetId(normalizedTarget, entryName, isDirectory);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await moveEntry(entryId, normalizedTarget);
      onSuccess(result.newId);
    } catch (e) {
      setError(t('moveError', { msg: localizeError(e) }));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">{t('moveTitle')}</h2>
        <p className="mt-1 text-sm text-gray-500">{t('moveHint')}</p>

        <div className="mt-3 grid grid-cols-2 gap-2 rounded-md bg-gray-50 p-2 text-xs text-gray-600">
          <div>
            <span className="font-semibold">{t('moveCurrent')}</span>
            <span className="block break-all font-mono">
              {currentDir || t('moveRoot')}
            </span>
          </div>
          <div>
            <span className="font-semibold">{t('moveTargetPreview')}</span>
            <span className="block break-all font-mono">
              {normalizedTarget || t('moveRoot')}
            </span>
          </div>
          <div className="col-span-2">
            <span className="font-semibold">{t('moveIdPreview')}</span>
            <span className="font-mono">
              {entryId} → {newId}
            </span>
          </div>
        </div>

        <div className="mt-4">
          <label className="block text-sm font-medium text-gray-700">
            {t('moveTargetLabel')}
          </label>
          <input
            value={targetDir}
            onChange={(e) => setTargetDir(e.target.value)}
            placeholder={t('moveTargetPlaceholder')}
            className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm font-mono"
          />
          <div className="mt-2 max-h-40 overflow-y-auto rounded-md border border-gray-200">
            <button
              type="button"
              onClick={() => setTargetDir('')}
              className={`block w-full px-2 py-1.5 text-left text-sm ${
                normalizedTarget === '' ? 'bg-blue-50 text-blue-700' : 'hover:bg-gray-50'
              }`}
            >
              {t('moveRoot')}
            </button>
            {directories.map((dir) => {
              const depth = dir.split('/').length - 1;
              const name = dir.split('/').pop() ?? dir;
              const active = normalizedTarget === dir;
              return (
                <button
                  key={dir}
                  type="button"
                  onClick={() => setTargetDir(dir)}
                  style={{ paddingLeft: `${8 + depth * 14}px` }}
                  className={`block w-full py-1.5 pr-2 text-left text-sm ${
                    active ? 'bg-blue-50 text-blue-700' : 'hover:bg-gray-50'
                  }`}
                  title={dir}
                >
                  <span className="text-gray-400">{depth === 0 ? '' : '└ '}</span>
                  {name}
                </button>
              );
            })}
          </div>
        </div>

        <p className="mt-3 text-xs text-gray-500">{t('moveConsistency')}</p>
        {attachmentCount > 0 && (
          <p className="mt-1 text-xs text-amber-700">
            {isDirectory
              ? t('moveAttachmentsDir', { count: attachmentCount })
              : t('moveAttachmentsFlat', { count: attachmentCount })}
          </p>
        )}

        {error && <p className="mt-3 text-xs text-red-600">{error}</p>}

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            disabled={busy}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100 disabled:opacity-50"
          >
            {t('moveCancel')}
          </button>
          <button
            disabled={busy || unchanged}
            onClick={submit}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {busy ? t('moveBusy') : t('moveSubmit')}
          </button>
        </div>
      </div>
    </div>
  );
}

interface FormatPreviewDialogProps {
  changes: Array<{ lineNo: number; before: string; after: string }>;
  changedLines: number;
  busy: boolean;
  onCancel: () => void;
  onApply: () => void;
}

/// 排版预览：只展示会变化的行，确认后才写盘，避免静默改写。
function FormatPreviewDialog({
  changes,
  changedLines,
  busy,
  onCancel,
  onApply,
}: FormatPreviewDialogProps) {
  const { t } = useTranslation('DetailPanel');
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="flex max-h-[80vh] w-full max-w-3xl flex-col rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">{t('formatTitle')}</h2>
        <p className="mt-1 text-xs text-gray-500">{t('formatHint')}</p>
        <p className="mt-2 text-xs font-medium text-blue-700">
          {t('formatChangedLines', { count: changedLines })}
        </p>

        <div className="mt-3 flex-1 overflow-y-auto">
          {changes.map((change) => (
            <div
              key={change.lineNo}
              className="mb-2 rounded-md border border-gray-200 p-2"
            >
              <div className="mb-1 text-[11px] text-gray-400">#{change.lineNo}</div>
              <div className="min-h-[1.25rem] whitespace-pre-wrap break-words rounded bg-red-50 px-2 py-1 font-mono text-xs text-red-800">
                {change.before}
              </div>
              <div className="mt-1 min-h-[1.25rem] whitespace-pre-wrap break-words rounded bg-green-50 px-2 py-1 font-mono text-xs text-green-800">
                {change.after}
              </div>
            </div>
          ))}
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onCancel}
            disabled={busy}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100 disabled:opacity-50"
          >
            {t('formatCancel')}
          </button>
          <button
            disabled={busy}
            onClick={onApply}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {busy ? t('saveBusy') : t('formatApply')}
          </button>
        </div>
      </div>
    </div>
  );
}

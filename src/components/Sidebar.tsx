import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import { usePanelStore } from '@/store/panelStore';
import { EntryDialog, type EntryDialogMode } from '@/components/EntryDialog';
import type { Entry } from '@shared/types';
import { SearchBox } from '@/search/SearchBox';

interface GroupNode {
  kind: 'group';
  /** 全路径，如 "compliance/gdpr" */
  fullPath: string;
  /** 当前段名，如 "gdpr" */
  name: string;
  children: TreeNode[];
}
interface EntryNode {
  kind: 'entry';
  entry: Entry;
}
type TreeNode = GroupNode | EntryNode;

const ROOT_NODE_KEY = '__root__';

export function Sidebar() {
  const { t } = useTranslation('Sidebar');
  const entries = useAppStore((s) => s.entries);
  const activeKbId = useAppStore((s) => s.activeKbId);
  const activeKbEntryCount = useAppStore((s) => s.activeKbEntryCount);
  const kbs = useAppStore((s) => s.kbs);
  const loadEntries = useAppStore((s) => s.loadEntries);
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);
  const selectEntry = useAppStore((s) => s.selectEntry);
  const openPanel = usePanelStore((s) => s.open);

  const handleSelect = (id: string) => {
    selectEntry(id);
    openPanel('detail');
  };

  const activeKb = useMemo(
    () => kbs.find((k) => k.id === activeKbId),
    [kbs, activeKbId]
  );

  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [dialogMode, setDialogMode] = useState<EntryDialogMode | null>(null);

  // 已存在的顶级分组（用于 dialog 选择）
  const topLevelGroups = useMemo(() => {
    const set = new Set<string>();
    for (const e of entries) {
      if (e.group) set.add(e.group.split('/')[0]);
    }
    return Array.from(set).sort();
  }, [entries]);

  const existingGroupsFull = useMemo(() => {
    const set = new Set<string>();
    for (const e of entries) if (e.group) set.add(e.group);
    return Array.from(set).sort();
  }, [entries]);

  /** 把 entries 构造为嵌套树。group 是 "compliance/gdpr" 这种路径式字符串。 */
  const tree = useMemo(() => buildTree(entries), [entries]);

  const toggleNode = (key: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  return (
    <aside className="flex w-72 flex-col border-r border-gray-200 bg-white">
      <div className="border-b border-gray-200 p-3">
        <div className="flex items-center justify-between gap-2">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5">
              <span className="text-sm">📁</span>
              <div className="truncate text-sm font-semibold text-gray-900">
                {activeKb ? activeKb.name : t('noKbTitle')}
              </div>
            </div>
            <div className="truncate text-xs text-gray-500">
              {activeKb
                ? `${activeKbEntryCount} ${activeKbEntryCount === 1 ? 'entry' : 'entries'} · ${activeKb.path}`
                : t('noKbPath')}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1">
            {activeKb && (
              <button
                onClick={() => setDialogMode({ kind: 'ungrouped' })}
                className="rounded-md bg-blue-500 px-2 py-1 text-xs font-medium text-white hover:bg-blue-600"
                title={t('createInRootTitle')}
              >
                +
              </button>
            )}
            <button
              onClick={() => openPanel('settings')}
              className="rounded-md border border-gray-300 bg-white px-2 py-1 text-xs text-gray-700 hover:bg-gray-50"
              title={t('manageKbTitle')}
            >
              ⚙
            </button>
          </div>
        </div>
        {activeKb ? (
          <button
            onClick={() => loadEntries()}
            className="mt-2 w-full rounded-md border border-gray-200 bg-white px-2 py-1 text-[11px] text-gray-500 hover:bg-gray-50"
            title={t('reloadTitle')}
          >
            {t('reloadButton')}
          </button>
        ) : null}
        <SearchBox />
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        {!activeKb ? (
          <div className="p-4 text-xs text-gray-500">
            {t('noKbYet')}
          </div>
        ) : tree.length === 0 ? (
          <div className="p-4 text-xs text-gray-500">
            {t('emptyKb')}
          </div>
        ) : (
          <ul className="space-y-0.5">
            {tree.map((node) => (
              <TreeRow
                key={node.kind === 'group' ? `g:${node.fullPath}` : `e:${node.entry.id}`}
                node={node}
                depth={0}
                collapsed={collapsed}
                onToggle={toggleNode}
                selectedEntryId={selectedEntryId}
                onSelect={handleSelect}
                onOpenDialog={setDialogMode}
              />
            ))}
          </ul>
        )}
      </div>

      {dialogMode && activeKb && (
        <EntryDialog
          activeKb={activeKb}
          mode={dialogMode}
          existingGroups={existingGroupsFull}
          topLevelGroups={topLevelGroups}
          onClose={() => setDialogMode(null)}
        />
      )}
    </aside>
  );
}

interface TreeRowProps {
  node: TreeNode;
  depth: number;
  collapsed: Set<string>;
  onToggle: (key: string) => void;
  selectedEntryId: string | null;
  onSelect: (id: string) => void;
  onOpenDialog: (mode: EntryDialogMode) => void;
}

function TreeRow({
  node,
  depth,
  collapsed,
  onToggle,
  selectedEntryId,
  onSelect,
  onOpenDialog,
}: TreeRowProps) {
  const { t } = useTranslation('Sidebar');
  if (node.kind === 'group') {
    const isCollapsed = collapsed.has(node.fullPath);
    return (
      <li>
        <div
          className="group flex items-center gap-1 rounded px-1 py-0.5 text-xs font-semibold uppercase tracking-wide text-gray-500 hover:bg-gray-100"
          style={{ paddingLeft: `${depth * 12 + 4}px` }}
        >
          <button
            onClick={() => onToggle(node.fullPath)}
            className="flex flex-1 items-center gap-1 truncate text-left"
          >
            <span className="w-3 text-gray-400">{isCollapsed ? '▸' : '▾'}</span>
            <span className="truncate">{node.name}</span>
            <span className="ml-1 text-gray-400">{countItems(node)}</span>
          </button>
          <button
            onClick={() =>
              onOpenDialog({ kind: 'in_group', parentGroup: node.fullPath })
            }
            className="invisible rounded px-1 text-sm font-bold text-blue-600 hover:bg-blue-100 group-hover:visible"
            title={t('createInGroupTitle', { path: node.fullPath })}
          >
            +
          </button>
        </div>
        {!isCollapsed && (
          <ul>
            {node.children.map((child) => (
              <TreeRow
                key={
                  child.kind === 'group'
                    ? `g:${child.fullPath}`
                    : `e:${child.entry.id}`
                }
                node={child}
                depth={depth + 1}
                collapsed={collapsed}
                onToggle={onToggle}
                selectedEntryId={selectedEntryId}
                onSelect={onSelect}
                onOpenDialog={onOpenDialog}
              />
            ))}
          </ul>
        )}
      </li>
    );
  }
  // entry
  const isSelected = selectedEntryId === node.entry.id;
  return (
    <li className="group/entry">
      <div
        className={`flex items-center gap-1 rounded text-sm transition ${
          isSelected
            ? 'bg-blue-50 font-medium text-blue-700'
            : 'text-gray-700 hover:bg-gray-100'
        }`}
        style={{ paddingLeft: `${depth * 12 + 4}px` }}
      >
        <button
          onClick={() => onSelect(node.entry.id)}
          className="flex min-w-0 flex-1 items-start gap-1 truncate py-0.5 pr-1 text-left"
        >
          <span className="mt-0.5 text-xs text-gray-400">📄</span>
          <span className="min-w-0 flex-1 truncate">{node.entry.title}</span>
        </button>
        <button
          onClick={() => onOpenDialog({ kind: 'as_child_of', parent: node.entry })}
          className="invisible mr-1 rounded px-1 text-xs font-bold text-blue-600 hover:bg-blue-100 group-hover/entry:visible"
          title={t('createChildTitle', { title: node.entry.title })}
        >
          +
        </button>
      </div>
    </li>
  );
}

function countItems(node: GroupNode): number {
  let n = 0;
  for (const c of node.children) {
    if (c.kind === 'entry') n += 1;
    else n += countItems(c);
  }
  return n;
}

function buildTree(entries: Entry[]): TreeNode[] {
  // 先按 group 分组
  const buckets = new Map<string, Entry[]>();
  const ungrouped: Entry[] = [];
  for (const e of entries) {
    if (!e.group) {
      ungrouped.push(e);
    } else {
      const arr = buckets.get(e.group) ?? [];
      arr.push(e);
      buckets.set(e.group, arr);
    }
  }

  // 收集所有出现过的 group path 段（包括中间段）
  const allPaths = new Set<string>();
  for (const g of buckets.keys()) {
    const parts = g.split('/');
    let acc = '';
    for (const p of parts) {
      acc = acc ? `${acc}/${p}` : p;
      allPaths.add(acc);
    }
  }

  // 递归构造 GroupNode
  function buildGroup(fullPath: string): GroupNode {
    const name = fullPath.split('/').pop() ?? fullPath;
    const directChildren: TreeNode[] = [];
    // 子 group：full path 以 `${fullPath}/` 开头
    const childPrefix = `${fullPath}/`;
    for (const p of allPaths) {
      if (p.startsWith(childPrefix) && !p.slice(childPrefix.length).includes('/')) {
        directChildren.push(buildGroup(p));
      }
    }
    // 直接挂在该 group 的 entries
    const directEntries = buckets.get(fullPath) ?? [];
    directEntries.sort((a, b) => a.title.localeCompare(b.title));
    for (const e of directEntries) directChildren.push({ kind: 'entry', entry: e });
    // 子 group 排序
    directChildren.sort((a, b) => {
      const aIsGroup = a.kind === 'group' ? 1 : 0;
      const bIsGroup = b.kind === 'group' ? 1 : 0;
      if (aIsGroup !== bIsGroup) return bIsGroup - aIsGroup;
      const aName = a.kind === 'group' ? a.name : a.entry.title;
      const bName = b.kind === 'group' ? b.name : b.entry.title;
      return aName.localeCompare(bName);
    });
    return { kind: 'group', fullPath, name, children: directChildren };
  }

  const rootGroups: TreeNode[] = [];
  for (const p of allPaths) {
    if (!p.includes('/')) rootGroups.push(buildGroup(p));
  }
  rootGroups.sort((a, b) =>
    a.kind === 'group' && b.kind === 'group' ? a.name.localeCompare(b.name) : 0
  );

  const ungroupedSorted = [...ungrouped].sort((a, b) => a.title.localeCompare(b.title));
  // ungrouped 放在最前面（group 排序其次）
  return [...rootGroups, ...ungroupedSorted.map((e) => ({ kind: 'entry' as const, entry: e }))];
}

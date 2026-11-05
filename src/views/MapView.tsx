import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import ReactFlow, {
  Background,
  Controls,
  Edge,
  Node,
  NodeChange,
  ReactFlowInstance,
} from 'reactflow';
import 'reactflow/dist/style.css';
import { useTranslation } from 'react-i18next';

import { useAppStore } from '@/store/useAppStore';
import { usePanelStore } from '@/store/panelStore';
import { NodeCard, NodeCardData } from '@/components/NodeCard';
import { ContextMenu, type MenuItem } from '@/components/ContextMenu';
import { mergeLayoutPositions } from './layoutSync';
import type { RelationType } from '@shared/types';

const RELATION_COLORS: Record<RelationType, string> = {
  reference: '#3B82F6',
  derived: '#10B981',
  contradicts: '#EF4444',
  supersedes: '#F97316',
  extends: '#A855F7',
  custom: '#6B7280',
};

const GROUP_PALETTE = [
  '#6366F1', // indigo
  '#10B981', // emerald
  '#F59E0B', // amber
  '#EC4899', // pink
  '#8B5CF6', // violet
  '#14B8A6', // teal
  '#F97316', // orange
  '#0EA5E9', // sky
  '#84CC16', // lime
  '#D946EF', // fuchsia
];

/** 稳定 hash → 0..n-1 */
function groupColorIndex(group: string, n: number): number {
  let h = 0;
  for (let i = 0; i < group.length; i++) {
    h = (h * 31 + group.charCodeAt(i)) | 0;
  }
  return Math.abs(h) % n;
}

const nodeTypes = { card: NodeCard };

const GROUP_COL_WIDTH = 5;
const NODE_W = 200;
const NODE_H = 110;
const GROUP_GAP_X = 80;
const GROUP_GAP_Y = 120;
const NODE_GAP_X = 28;
const NODE_GAP_Y = 36;

interface GroupLayout {
  /// 物理目录（相对 KB 根）；仅用于空间聚类，展示分组用 NodeCardData.group
  dir: string;
  cols: number;
  rows: number;
  origin: { x: number; y: number };
  accent: string;
}

export function MapView() {
  const { t } = useTranslation('MapView');
  const { t: tCm } = useTranslation('ContextMenu');
  const entries = useAppStore((s) => s.entries);
  const selectEntry = useAppStore((s) => s.selectEntry);
  const deleteEntry = useAppStore((s) => s.deleteEntry);
  const deleteRelation = useAppStore((s) => s.deleteRelation);
  const setRenamingEntryId = useAppStore((s) => s.setRenamingEntryId);
  const relationFilter = useAppStore((s) => s.relationFilter);
  const setDraggingNode = useAppStore((s) => s.setDraggingNode);
  const activeKbId = useAppStore((s) => s.activeKbId);
  // entries 每次因 watcher/写命令刷新都会递增；重命名会迁移后端布局键
  // （`layout_store::rename_key`），前端必须重新拉取布局，否则新 id 在
  // 本地 positions 里没有坐标，节点会被当作“新节点”重新布局。
  const entriesRevision = useAppStore((s) => s.entriesRevision);
  const openPanel = usePanelStore((s) => s.open);

  // 用户拖拽后的节点位置（entry id → 坐标）。初始值来自 KB 根目录的
  // `.synapse-layout.json`；没有记录的节点走分组网格的默认位置。
  const [positions, setPositions] = useState<Record<string, { x: number; y: number }>>({});
  // 本次 KB 的布局是否已读完（成功或失败回退都算"就绪"）。只有就绪后才
  // 允许做首次 fitView，否则会按默认网格算一次边界、布局到达后再跳一次。
  const [layoutReady, setLayoutReady] = useState(false);
  const flowInstanceRef = useRef<ReactFlowInstance | null>(null);
  // 每个 KB 加载周期只自动 fit 一次；手动「适配视图」按钮不受影响。
  const autoFittedRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    setPositions({});
    setLayoutReady(false);
    autoFittedRef.current = false;
    if (!activeKbId) {
      // 没有活动 KB：不挂起，直接视为就绪（图谱此时是空态）。
      setLayoutReady(true);
      return () => {
        cancelled = true;
      };
    }
    void invoke<Record<string, { x: number; y: number }>>('get_layout')
      .then((loaded) => {
        if (cancelled) return;
        setPositions(loaded ?? {});
        setLayoutReady(true);
      })
      .catch(() => {
        // 布局读取失败时退回默认分组网格，不阻塞图谱渲染。
        if (!cancelled) setLayoutReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, [activeKbId]);

  // 从磁盘重新读取布局并合并进内存。拖动结束落库成功后会再调用一次：
  // 拖拽期间若发生 entries 刷新（重命名/外部增删），上面的 effect 会跳过，
  // 这里保证不会因为跳过而永久丢失磁盘上的新坐标。
  const syncLayoutFromDisk = useCallback(async () => {
    if (!activeKbId) return;
    try {
      const loaded = await invoke<Record<string, { x: number; y: number }>>('get_layout');
      setPositions((prev) => mergeLayoutPositions(prev, loaded));
    } catch {
      // 读取失败保持内存坐标，等待下次刷新。
    }
  }, [activeKbId]);

  // 条目标识变化（重命名/外部增删）后同步磁盘布局：
  // - 后端 `execute_rename` 已经把 `.synapse-layout.json` 里的旧键迁移到新 id
  //   （commands.rs 的 `layout_store::rename_key`），这里只是把新布局读回内存；
  // - 用合并而不是整体替换，保留用户当前拖拽中的内存坐标；
  // - 不做 `fitView`，避免刷新时画面跳一下。
  // 注意依赖里不放 `isDraggingNode`：拖拽结束会立刻 save_node_position，
  // 若此时触发读盘会因竞态读回旧坐标，把刚拖好的节点弹回去。拖拽期间若
  // 真有 revision 变化则跳过，留待下次刷新。
  useEffect(() => {
    if (!activeKbId || !layoutReady) return;
    if (useAppStore.getState().isDraggingNode) return;
    let cancelled = false;
    void invoke<Record<string, { x: number; y: number }>>('get_layout')
      .then((loaded) => {
        if (cancelled || !loaded) return;
        setPositions((prev) => mergeLayoutPositions(prev, loaded));
      })
      .catch(() => {
        // 读取失败保持内存坐标，等待下次刷新。
      });
    return () => {
      cancelled = true;
    };
  }, [activeKbId, entriesRevision, layoutReady]);

  // 布局与节点都就绪后自动适配一次视图：重启/切换 KB 时无需手动点按钮。
  // 放在两次 rAF 之后，确保 React Flow 已按最新 position 完成节点测量，
  // 否则 fitView 会按旧（或未知）尺寸算错边界。events 变化（重命名、增删）
  // 不会再触发，避免画面抖动和拖拽回弹。
  useEffect(() => {
    if (!layoutReady || autoFittedRef.current) return;
    if (!flowInstanceRef.current || entries.length === 0) return;
    autoFittedRef.current = true;
    let raf2 = 0;
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => {
        flowInstanceRef.current?.fitView({ padding: 0.2, duration: 0 });
      });
    });
    return () => {
      cancelAnimationFrame(raf1);
      cancelAnimationFrame(raf2);
    };
  }, [layoutReady, entries.length]);

  const [contextMenu, setContextMenu] = useState<
    | { x: number; y: number; kind: 'node'; entryId: string }
    | { x: number; y: number; kind: 'edge'; source: string; target: string; type: RelationType }
    | null
  >(null);
  const closeContextMenu = useCallback(() => setContextMenu(null), []);

  const copyToClipboard = useCallback(
    async (text: string) => {
      try {
        await navigator.clipboard.writeText(text);
      } catch {
        // ignore — clipboard may be unavailable in some webviews
      }
    },
    []
  );

  // 1) 按物理目录 `dir` 分桶（保持首次出现顺序）。
  //    `group` 现在只是顶层展示分类，用它聚类会把不同子目录混在一起；
  //    这里用 `dir` 保持图谱的空间组织与解耦前一致，节点卡片副标题
  //    仍显示 `group`（顶层分类）。
  const groupOrder = useMemo(() => {
    const order: string[] = [];
    const seen = new Set<string>();
    for (const e of entries) {
      const g = e.dir ?? '__root__';
      if (!seen.has(g)) {
        seen.add(g);
        order.push(g);
      }
    }
    return order;
  }, [entries]);

  // 2) 每个目录内布局计算 + 颜色
  const groupLayouts = useMemo<GroupLayout[]>(() => {
    return groupOrder.map((g) => {
      const items = entries.filter((e) => (e.dir ?? '__root__') === g);
      const cols = Math.min(items.length, GROUP_COL_WIDTH);
      const rows = Math.max(1, Math.ceil(items.length / GROUP_COL_WIDTH));
      return {
        dir: g,
        cols,
        rows,
        origin: { x: 0, y: 0 },
        accent: GROUP_PALETTE[groupColorIndex(g, GROUP_PALETTE.length)],
      };
    });
  }, [entries, groupOrder]);

  // 3) 算出每个目录的 origin + 每个 entry 的位置
  const nodes: Node<NodeCardData>[] = useMemo(() => {
    let cursorY = 0;
    const positionsByGroup = new Map<string, { x: number; y: number }>();
    for (const layout of groupLayouts) {
      positionsByGroup.set(layout.dir, { x: 0, y: cursorY });
      cursorY += layout.rows * NODE_H + (layout.rows - 1) * NODE_GAP_Y + GROUP_GAP_Y;
    }

    const result: Node<NodeCardData>[] = [];
    for (const layout of groupLayouts) {
      const items = entries.filter((e) => (e.dir ?? '__root__') === layout.dir);
      const groupOrigin = positionsByGroup.get(layout.dir)!;
      items.forEach((entry, i) => {
        const col = i % layout.cols;
        const row = Math.floor(i / layout.cols);
        const x = groupOrigin.x + col * (NODE_W + NODE_GAP_X);
        const y = groupOrigin.y + row * (NODE_H + NODE_GAP_Y);
        result.push({
          id: entry.id,
          type: 'card',
          position: positions[entry.id] ?? { x, y },
          data: {
            entryId: entry.id,
            title: entry.title,
            group: entry.group,
            hasContentMd: entry.hasContentMd,
            accent: layout.accent,
            errorCount: entry.errorCount,
            warningCount: entry.warningCount,
          },
        });
      });
    }
    return result;
  }, [entries, groupLayouts, positions]);

  const edges: Edge[] = useMemo(() => {
    const entryIds = new Set(entries.map((e) => e.id));
    const result: Edge[] = [];
    for (const entry of entries) {
      for (const rel of entry.relations) {
        if (!relationFilter.has(rel.type as RelationType)) continue;
        if (!entryIds.has(rel.toId)) continue;
        result.push({
          id: `${entry.id}-${rel.toId}-${rel.type}`,
          source: entry.id,
          target: rel.toId,
          label: rel.type,
          style: {
            stroke: RELATION_COLORS[rel.type as RelationType] || RELATION_COLORS.custom,
            strokeWidth: 1.5,
          },
          labelStyle: { fontSize: 10, fill: '#6B7280' },
          animated: rel.type === 'derived',
        });
      }
    }
    return result;
  }, [entries, relationFilter]);

  // 拖拽过程中的位置变化只更新内存状态，保证节点跟手；落库发生在
  // `onNodeDragStop`，避免每帧都写一次 `.synapse-layout.json`。
  const onNodesChange = useCallback((changes: NodeChange[]) => {
    setPositions((prev) => {
      let next: Record<string, { x: number; y: number }> | null = null;
      for (const change of changes) {
        if (change.type !== 'position' || !change.position) continue;
        next ??= { ...prev };
        next[change.id] = { x: change.position.x, y: change.position.y };
      }
      return next ?? prev;
    });
  }, []);

  const onNodeDragStart = useCallback(() => {
    setDraggingNode(true);
  }, [setDraggingNode]);

  const onNodeDragStop = useCallback(
    (_: React.MouseEvent, node: Node) => {
      setDraggingNode(false);
      void invoke('save_node_position', {
        entryId: node.id,
        x: node.position.x,
        y: node.position.y,
      })
        .then(() => {
          // 补偿：拖拽期间被跳过的 entries 刷新可能改过布局（重命名迁移
          // 键、外部增删），落库成功后重新对齐一次，避免永久丢坐标。
          void syncLayoutFromDisk();
        })
        .catch(() => {
          // 持久化失败只影响下次启动的默认位置，不打断当前交互；
          // 此时不拉取磁盘布局，避免把节点弹回旧坐标。
        });
    },
    [setDraggingNode, syncLayoutFromDisk]
  );

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      selectEntry(node.id);
      openPanel('detail');
    },
    [selectEntry, openPanel]
  );

  const onNodeContextMenu = useCallback(
    (event: React.MouseEvent, node: Node) => {
      event.preventDefault();
      setContextMenu({ x: event.clientX, y: event.clientY, kind: 'node', entryId: node.id });
    },
    []
  );

  const onEdgeContextMenu = useCallback(
    (event: React.MouseEvent, edge: Edge) => {
      event.preventDefault();
      const type = (edge.label ?? 'custom') as RelationType;
      setContextMenu({
        x: event.clientX,
        y: event.clientY,
        kind: 'edge',
        source: edge.source,
        target: edge.target,
        type,
      });
    },
    []
  );

  const buildNodeMenuItems = useCallback(
    (entryId: string): MenuItem[] => {
      const entry = entries.find((e) => e.id === entryId);
      return [
        {
          label: tCm('open'),
          onClick: () => {
            selectEntry(entryId);
            openPanel('detail');
          },
        },
        {
          label: tCm('copyId'),
          onClick: () => {
            void copyToClipboard(entryId);
          },
        },
        {
          label: tCm('copyWikilink'),
          onClick: () => {
            void copyToClipboard(`[[${entryId}]]`);
          },
        },
        { label: '', onClick: () => {}, separator: true },
        {
          label: tCm('rename'),
          onClick: () => {
            // Open detail panel first, then ask DetailPanel to open the
            // rename dialog. DetailPanel watches renamingEntryId.
            selectEntry(entryId);
            openPanel('detail');
            setRenamingEntryId(entryId);
          },
        },
        {
          label: tCm('findReferences'),
          onClick: () => {
            // Simple v1: count incoming/outgoing and alert. The DetailPanel
            // already shows these — we just jump to it.
            selectEntry(entryId);
            openPanel('detail');
          },
        },
        { label: '', onClick: () => {}, separator: true },
        {
          label: tCm('delete'),
          danger: true,
          onClick: () => {
            const title = entry?.title ?? entryId;
            if (window.confirm(tCm('deleteConfirmEntry', { title }))) {
              void deleteEntry(entryId);
            }
          },
        },
      ];
    },
    [entries, tCm, selectEntry, openPanel, copyToClipboard, setRenamingEntryId, deleteEntry]
  );

  const buildEdgeMenuItems = useCallback(
    (source: string, target: string, type: RelationType): MenuItem[] => {
      return [
        {
          label: tCm('deleteRelation'),
          danger: true,
          onClick: () => {
            if (window.confirm(tCm('deleteConfirmRelation'))) {
              void deleteRelation(source, target, type);
            }
          },
        },
      ];
    },
    [tCm, deleteRelation]
  );

  if (entries.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-gray-500">
        {t('empty')}
      </div>
    );
  }

  return (
    <div className="relative h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onNodeDragStart={onNodeDragStart}
        onNodeDragStop={onNodeDragStop}
        onNodeClick={onNodeClick}
        onNodeContextMenu={onNodeContextMenu}
        onEdgeContextMenu={onEdgeContextMenu}
        onInit={(instance) => {
          flowInstanceRef.current = instance;
        }}
        minZoom={0.1}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
      >
        <Background gap={16} size={1} color="#E5E7EB" />
        <Controls />
      </ReactFlow>

      {contextMenu && contextMenu.kind === 'node' && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={buildNodeMenuItems(contextMenu.entryId)}
          onClose={closeContextMenu}
        />
      )}
      {contextMenu && contextMenu.kind === 'edge' && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={buildEdgeMenuItems(contextMenu.source, contextMenu.target, contextMenu.type)}
          onClose={closeContextMenu}
        />
      )}

      {/* Group legend */}
      <div className="pointer-events-none absolute right-3 top-3 z-10 max-h-[calc(100%-2rem)] w-56 overflow-y-auto rounded-md border border-gray-200 bg-white/95 p-2 text-xs shadow-sm backdrop-blur">
        <div className="mb-1 font-semibold text-gray-700">{t('legendTitle')}</div>
        <ul className="space-y-0.5">
          {groupOrder.map((g) => {
            const layout = groupLayouts.find((l) => l.dir === g)!;
            const count = entries.filter((e) => (e.dir ?? '__root__') === g).length;
            return (
              <li key={g} className="flex items-center gap-2 truncate">
                <span
                  className="inline-block h-3 w-3 shrink-0 rounded-sm"
                  style={{ backgroundColor: layout.accent }}
                />
                <span className="truncate" title={g}>
                  {g === '__root__' ? t('rootGroup') : g}
                </span>
                <span className="ml-auto text-gray-400">{count}</span>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
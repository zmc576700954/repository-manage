import { useCallback, useMemo } from 'react';
import ReactFlow, {
  Background,
  Controls,
  Edge,
  Node,
  NodeChange,
} from 'reactflow';
import 'reactflow/dist/style.css';
import { useTranslation } from 'react-i18next';

import { useAppStore } from '@/store/useAppStore';
import { usePanelStore } from '@/store/panelStore';
import { NodeCard, NodeCardData } from '@/components/NodeCard';
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
  group: string;
  cols: number;
  rows: number;
  origin: { x: number; y: number };
  accent: string;
}

export function MapView() {
  const { t } = useTranslation('MapView');
  const entries = useAppStore((s) => s.entries);
  const selectEntry = useAppStore((s) => s.selectEntry);
  const relationFilter = useAppStore((s) => s.relationFilter);
  const openPanel = usePanelStore((s) => s.open);

  // 1) 按 group 分桶（保持 group 顺序，按 entries 中首次出现的顺序）
  const groupOrder = useMemo(() => {
    const order: string[] = [];
    const seen = new Set<string>();
    for (const e of entries) {
      const g = e.group ?? '__root__';
      if (!seen.has(g)) {
        seen.add(g);
        order.push(g);
      }
    }
    return order;
  }, [entries]);

  // 2) 每个 group 内布局计算 + 颜色
  const groupLayouts = useMemo<GroupLayout[]>(() => {
    return groupOrder.map((g, idx) => {
      const items = entries.filter((e) => (e.group ?? '__root__') === g);
      const cols = Math.min(items.length, GROUP_COL_WIDTH);
      const rows = Math.max(1, Math.ceil(items.length / GROUP_COL_WIDTH));
      return {
        group: g,
        cols,
        rows,
        origin: { x: 0, y: 0 },
        accent: GROUP_PALETTE[groupColorIndex(g, GROUP_PALETTE.length)],
      };
    });
  }, [entries, groupOrder]);

  // 3) 算出每个 group 的 origin + 每个 entry 的位置
  const nodes: Node<NodeCardData>[] = useMemo(() => {
    let cursorY = 0;
    const positionsByGroup = new Map<string, { x: number; y: number }>();
    for (const layout of groupLayouts) {
      positionsByGroup.set(layout.group, { x: 0, y: cursorY });
      cursorY += layout.rows * NODE_H + (layout.rows - 1) * NODE_GAP_Y + GROUP_GAP_Y;
    }

    const result: Node<NodeCardData>[] = [];
    for (const layout of groupLayouts) {
      const items = entries.filter((e) => (e.group ?? '__root__') === layout.group);
      const groupOrigin = positionsByGroup.get(layout.group)!;
      items.forEach((entry, i) => {
        const col = i % layout.cols;
        const row = Math.floor(i / layout.cols);
        const x = groupOrigin.x + col * (NODE_W + NODE_GAP_X);
        const y = groupOrigin.y + row * (NODE_H + NODE_GAP_Y);
        result.push({
          id: entry.id,
          type: 'card',
          position: { x, y },
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
  }, [entries, groupLayouts]);

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

  const onNodesChange = useCallback(
    (changes: NodeChange[]) => {
      return changes;
    },
    []
  );

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      selectEntry(node.id);
      openPanel('detail');
    },
    [selectEntry, openPanel]
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
        onNodeClick={onNodeClick}
        fitView
        minZoom={0.1}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
      >
        <Background gap={16} size={1} color="#E5E7EB" />
        <Controls />
      </ReactFlow>

      {/* Group legend */}
      <div className="pointer-events-none absolute right-3 top-3 z-10 max-h-[calc(100%-2rem)] w-56 overflow-y-auto rounded-md border border-gray-200 bg-white/95 p-2 text-xs shadow-sm backdrop-blur">
        <div className="mb-1 font-semibold text-gray-700">{t('legendTitle')}</div>
        <ul className="space-y-0.5">
          {groupOrder.map((g) => {
            const layout = groupLayouts.find((l) => l.group === g)!;
            const count = entries.filter((e) => (e.group ?? '__root__') === g).length;
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
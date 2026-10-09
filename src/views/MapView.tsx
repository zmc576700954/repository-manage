import { useCallback, useMemo } from 'react';
import ReactFlow, {
  Background,
  Controls,
  Edge,
  Node,
  NodeChange,
} from 'reactflow';
import 'reactflow/dist/style.css';

import { useAppStore } from '@/store/useAppStore';
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

const nodeTypes = { card: NodeCard };

const DEFAULT_LAYOUT: Record<string, { x: number; y: number }> = {};

export function MapView() {
  const entries = useAppStore((s) => s.entries);
  const selectEntry = useAppStore((s) => s.selectEntry);
  const relationFilter = useAppStore((s) => s.relationFilter);

  const nodes: Node<NodeCardData>[] = useMemo(() => {
    return entries.map((entry, i) => {
      const fallback = DEFAULT_LAYOUT[entry.id] || {
        x: (i % 5) * 220,
        y: Math.floor(i / 5) * 140,
      };
      return {
        id: entry.id,
        type: 'card',
        position: fallback,
        data: {
          title: entry.title,
          group: entry.group,
          hasContentMd: entry.hasContentMd,
        },
      };
    });
  }, [entries]);

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
    },
    [selectEntry]
  );

  if (entries.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-gray-500">
        No entries yet. Configure a knowledge base root in Settings.
      </div>
    );
  }

  return (
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
  );
}
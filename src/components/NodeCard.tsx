import { Handle, NodeProps, Position } from 'reactflow';

export interface NodeCardData {
  title: string;
  group?: string;
  hasContentMd: boolean;
}

export function NodeCard({ data, selected }: NodeProps<NodeCardData>) {
  return (
    <div
      className={`min-w-[160px] rounded-lg border bg-white px-3 py-2 shadow-sm transition ${
        selected
          ? 'border-blue-500 ring-2 ring-blue-200'
          : 'border-gray-200 hover:border-gray-300'
      }`}
    >
      <Handle type="target" position={Position.Top} className="!bg-gray-300" />
      <div className="flex items-center gap-2">
        {!data.hasContentMd && (
          <span className="text-amber-500" title="Missing content.md">
            ⚠
          </span>
        )}
        <span className="text-sm font-medium text-gray-900">{data.title}</span>
      </div>
      {data.group && (
        <div className="mt-1 text-xs text-gray-500">{data.group}</div>
      )}
      <Handle type="source" position={Position.Bottom} className="!bg-gray-300" />
    </div>
  );
}
import { Handle, NodeProps, Position } from 'reactflow';
import { useTranslation } from 'react-i18next';

export interface NodeCardData {
  title: string;
  group?: string;
  hasContentMd: boolean;
  accent?: string;
  errorCount?: number;
  warningCount?: number;
}

export function NodeCard({ data, selected }: NodeProps<NodeCardData>) {
  const { t } = useTranslation('NodeCard');
  const accent = data.accent ?? '#6B7280';
  const hasError = (data.errorCount ?? 0) > 0;
  const hasWarning = (data.warningCount ?? 0) > 0;
  const validationBorder = hasError
    ? 'border-2 border-red-500'
    : hasWarning
    ? 'border-2 border-yellow-400'
    : '';
  const badge = hasError
    ? { color: 'bg-red-500', title: t('errorTooltip', { count: data.errorCount }) }
    : hasWarning
    ? { color: 'bg-yellow-400', title: t('warningTooltip', { count: data.warningCount }) }
    : null;
  return (
    <div
      className={`relative max-w-[200px] min-w-[160px] overflow-hidden rounded-lg border bg-white px-3 py-2 shadow-sm transition ${validationBorder} ${
        selected
          ? 'border-blue-500 ring-2 ring-blue-200'
          : 'border-gray-200 hover:border-gray-300'
      }`}
      style={{ borderLeft: `4px solid ${accent}` }}
    >
      {badge && (
        <span
          className={`absolute -right-1 -top-1 flex h-4 w-4 items-center justify-center rounded-full text-[10px] font-bold text-white ${badge.color}`}
          title={badge.title}
        >
          !
        </span>
      )}
      <Handle type="target" position={Position.Top} className="!bg-gray-300" />
      <div className="flex min-w-0 items-center gap-2">
        {!data.hasContentMd && (
          <span className="shrink-0 text-amber-500" title={t('missingContentTitle')}>
            ⚠
          </span>
        )}
        <span className="truncate text-sm font-medium text-gray-900" title={data.title}>
          {data.title}
        </span>
      </div>
      {data.group && (
        <div className="mt-1 truncate text-xs text-gray-500" title={data.group}>
          📁 {data.group}
        </div>
      )}
      <Handle type="source" position={Position.Bottom} className="!bg-gray-300" />
    </div>
  );
}
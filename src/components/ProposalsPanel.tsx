import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import type { ProposalDetail, ProposalMeta } from '@shared/types';
import { lineDiff } from '@/utils/lineDiff';
import type { DiffLine } from '@/utils/lineDiff';

const STATUS_STYLE: Record<string, string> = {
  pending: 'bg-amber-100 text-amber-800',
  reviewed: 'bg-blue-100 text-blue-800',
  applied: 'bg-green-100 text-green-800',
  rejected: 'bg-gray-200 text-gray-500',
};

export const ProposalsPanel: React.FC = () => {
  const { t } = useTranslation('ProposalsPanel');
  const activeKbId = useAppStore((s) => s.activeKbId);
  const loadEntries = useAppStore((s) => s.loadEntries);
  const [items, setItems] = useState<
    (ProposalMeta & { reviewVerdict: string | null })[]
  >([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<ProposalDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    setError(null);
    try {
      const list = await invoke<ProposalMeta[]>('list_proposals');
      const enriched: (ProposalMeta & { reviewVerdict: string | null })[] = [];
      for (const m of list) {
        if (m.status !== 'reviewed') {
          enriched.push({ ...m, reviewVerdict: null });
          continue;
        }
        const d = await invoke<ProposalDetail>('read_proposal', { id: m.id });
        const vLine = (d.review ?? '')
          .split('\n')
          .find((line) => line.startsWith('verdict:'));
        const verdict = vLine ? vLine.replace('verdict:', '').trim() : null;
        enriched.push({ ...m, reviewVerdict: verdict });
      }
      setItems(enriched);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const loadDetail = useCallback(async (id: string) => {
    setError(null);
    setDetail(null);
    try {
      setDetail(await invoke<ProposalDetail>('read_proposal', { id }));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload, activeKbId]);

  const onApply = async () => {
    if (!detail) return;
    setBusy(true);
    setError(null);
    try {
      await invoke('apply_proposal', { id: detail.id });
      setDetail(null);
      setSelectedId(null);
      await reload();
      await loadEntries();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onDismiss = async () => {
    if (!detail) return;
    setBusy(true);
    setError(null);
    try {
      await invoke('dismiss_proposal', { id: detail.id });
      setDetail(null);
      setSelectedId(null);
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between px-3 py-2">
        <span className="text-xs text-gray-500">
          {t('count', { count: items.length })}
        </span>
        <button
          onClick={() => void reload()}
          className="rounded border border-gray-200 px-2 py-0.5 text-xs text-gray-600 hover:bg-gray-50"
        >
          {t('reload')}
        </button>
      </div>
      {error && (
        <div className="mx-3 mb-2 rounded bg-red-50 px-2 py-1 text-xs text-red-700">
          {error}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
        {items.length === 0 ? (
          <p className="mt-6 text-center text-xs text-gray-400">{t('empty')}</p>
        ) : (
          <ul className="space-y-1.5">
            {items.map((m) => (
              <ProposalRow
                key={m.id}
                meta={m}
                selected={selectedId === m.id}
                onSelect={() => {
                  setSelectedId(m.id);
                  void loadDetail(m.id);
                }}
              />
            ))}
          </ul>
        )}
        {detail && (
          <ProposalDetailCard
            detail={detail}
            busy={busy}
            onApply={() => void onApply()}
            onDismiss={() => void onDismiss()}
          />
        )}
      </div>
    </div>
  );
};

interface RowProps {
  meta: ProposalMeta & { reviewVerdict: string | null };
  selected: boolean;
  onSelect: () => void;
}

const ProposalRow: React.FC<RowProps> = ({ meta, selected, onSelect }) => {
  const { t } = useTranslation('ProposalsPanel');
  return (
    <li>
      <button
        onClick={onSelect}
        className={`w-full rounded-md border px-2.5 py-2 text-left text-xs ${
          selected
            ? 'border-blue-400 bg-blue-50'
            : 'border-gray-200 bg-white hover:bg-gray-50'
        }`}
      >
        <div className="flex items-center justify-between gap-2">
          <span className="truncate font-medium text-gray-800">{meta.id}</span>
          <span
            className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] ${
              STATUS_STYLE[meta.status] ?? 'bg-gray-100 text-gray-600'
            }`}
          >
            {t(`status_${meta.status}`, { defaultValue: meta.status })}
            {meta.reviewVerdict
              ? ` · ${t(`verdict_${meta.reviewVerdict}`, { defaultValue: meta.reviewVerdict })}`
              : ''}
          </span>
        </div>
        <div className="mt-1 truncate text-gray-500">{meta.target}</div>
        {meta.rationale && (
          <div className="mt-0.5 truncate text-gray-400">{meta.rationale}</div>
        )}
      </button>
    </li>
  );
};

const ProposalDetailCard: React.FC<{
  detail: ProposalDetail;
  busy: boolean;
  onApply: () => void;
  onDismiss: () => void;
}> = ({ detail, busy, onApply, onDismiss }) => {
  const { t } = useTranslation('ProposalsPanel');
  return (
    <div className="mt-3 rounded-md border border-gray-200 p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-xs font-semibold text-gray-800">
          {detail.id}
        </span>
        <span
          className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] ${
            STATUS_STYLE[detail.status] ?? 'bg-gray-100 text-gray-600'
          }`}
        >
          {t(`status_${detail.status}`, { defaultValue: detail.status })}
        </span>
      </div>
      <div className="mt-1 text-xs text-gray-500">
        {t('targetLabel')}: {detail.targetId ?? (
          <span className="text-red-600">{t('targetUnresolved')}</span>
        )}
      </div>
      {detail.rationale && (
        <p className="mt-1 text-xs text-gray-600">{detail.rationale}</p>
      )}
      {detail.review && (
        <div className="mt-2 rounded bg-blue-50/60 p-2">
          <p className="text-[10px] font-medium text-blue-800">
            {t('reviewLabel')}
          </p>
          <pre className="mt-1 whitespace-pre-wrap text-[11px] leading-relaxed text-gray-700">
            {detail.review}
          </pre>
        </div>
      )}
      {detail.content == null ? (
        <p className="mt-2 text-xs text-gray-500">{t('noContentBlock')}</p>
      ) : detail.currentContent == null ? (
        <p className="mt-2 text-xs text-red-600">{t('noCurrentContent')}</p>
      ) : (
        <>
          <DiffView current={detail.currentContent} proposed={detail.content} />
          <div className="mt-3 flex items-center gap-2">
            <button
              disabled={busy}
              onClick={onApply}
              className="rounded bg-blue-500 px-3 py-1 text-xs font-medium text-white hover:bg-blue-600 disabled:opacity-50"
            >
              {t('applyButton')}
            </button>
            <button
              disabled={busy}
              onClick={onDismiss}
              className="rounded border border-gray-300 bg-white px-3 py-1 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50"
            >
              {t('dismissButton')}
            </button>
            <span className="text-[10px] text-gray-400">{t('applyHint')}</span>
          </div>
        </>
      )}
    </div>
  );
};

const DiffView: React.FC<{ current: string; proposed: string }> = ({
  current,
  proposed,
}) => {
  const { t } = useTranslation('ProposalsPanel');
  const lines: DiffLine[] = lineDiff(current, proposed);
  const added = lines.filter((l) => l.type === 'add').length;
  const removed = lines.filter((l) => l.type === 'del').length;
  return (
    <div className="mt-2">
      <div className="mb-1 text-[10px] text-gray-400">
        {t('diffSummary', { added, removed })}
      </div>
      <pre className="max-h-72 overflow-y-auto rounded bg-gray-950 p-2 font-mono text-[11px] leading-snug">
        {lines.map((l, idx) => (
          <div
            key={idx}
            className={
              l.type === 'add'
                ? 'bg-green-900/60 text-green-200'
                : l.type === 'del'
                  ? 'bg-red-900/60 text-red-200'
                  : 'text-gray-300'
            }
          >
            {l.type === 'add' ? '+ ' : l.type === 'del' ? '- ' : '  '}
            {l.text}
          </div>
        ))}
      </pre>
    </div>
  );
};

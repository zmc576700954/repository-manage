import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { MarkdownView } from '@/markdown/MarkdownView';
import { useAppStore } from '@/store/useAppStore';

/**
 * KB 首页（D6）：根 README 作为特殊入口展示——可读、不入图谱、
 * 不索引为条目、不参与关系解析。README 里的 wikilink 按 broken-ref
 * 渲染为纯文本（首页只读，不建关系）。
 */
export const KbHomePanel: React.FC = () => {
  const { t } = useTranslation('KbHomePanel');
  const [content, setContent] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const activeKbId = useAppStore((s) => s.activeKbId);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    invoke<string | null>('read_kb_readme')
      .then((c: string | null) => {
        if (!cancelled) setContent(c);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [activeKbId]);

  if (error) {
    return <div className="p-4 text-xs text-red-600">{error}</div>;
  }
  if (content == null) {
    return <div className="p-4 text-xs text-gray-500">{t('noReadme')}</div>;
  }
  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto px-4 py-3">
        <MarkdownView
          source={content}
          resolveTarget={() => null}
          onNavigate={() => {}}
          onCreateFromRef={() => {}}
        />
      </div>
    </div>
  );
};

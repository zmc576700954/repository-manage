import { useEffect, useMemo, useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { remarkWikilink } from './remarkWikilink';
import type { Components } from 'react-markdown';

export interface MarkdownViewProps {
  /** The raw markdown source. */
  source: string;
  /**
   * Resolve a wikilink target id. Returns target info if it exists
   * in the current KB; otherwise null to indicate a broken reference.
   */
  resolveTarget: (id: string) => { id: string; title: string } | null;
  /**
   * Click handler for an existing target: navigate to it.
   */
  onNavigate: (id: string) => void;
  /**
   * Click handler for a broken target: open "create from ref" dialog.
   */
  onCreateFromRef: (id: string, caption: string) => void;
}

/** Render markdown into React nodes, with [[wikilink]] handling. */
export function MarkdownView({
  source,
  resolveTarget,
  onNavigate,
  onCreateFromRef,
}: MarkdownViewProps) {
  const { t } = useTranslation('MarkdownView');
  const [renderError, setRenderError] = useState(false);

  // Reset error state when source changes
  useEffect(() => {
    setRenderError(false);
  }, [source]);

  const components = useMemo(() => {
    return {
      wikilink(props: { node?: any; data?: { id: string; caption: string }; children?: React.ReactNode }) {
        const id = props.data?.id ?? '';
        const caption = props.data?.caption ?? id;
        if (!id) return <>{caption}</>;
        const target = resolveTarget(id);
        const handleClick = (e: React.MouseEvent) => {
          e.preventDefault();
          if (target) onNavigate(id);
          else onCreateFromRef(id, caption);
        };
        if (!target) {
          return (
            <a
              href="#"
              onClick={handleClick}
              className="cursor-pointer text-red-700 decoration-dotted"
              title={t('wikilinkBrokenHint')}
            >
              {caption}
              <span className="ml-1 text-[10px] text-red-500">
                {t('wikilinkBroken')}
              </span>
            </a>
          );
        }
        return (
          <a
            href="#"
            onClick={handleClick}
            className="cursor-pointer text-blue-600 underline decoration-blue-300 hover:decoration-blue-600"
          >
            {caption}
          </a>
        );
      },
      // Allow raw HTML in markdown (default react-markdown strips it; we keep GFM only)
    } as unknown as Components;
  }, [resolveTarget, onNavigate, onCreateFromRef, t]);

  const handleError = useCallback((e: { error: Error }) => {
    console.error('MarkdownView render error:', e.error);
    setRenderError(true);
  }, []);

  if (renderError) {
    return (
      <pre className="max-h-[60vh] overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800">
        {source}
      </pre>
    );
  }

  try {
    return (
      <div className="prose prose-sm max-w-none break-words text-gray-800">
        <ReactMarkdown
          remarkPlugins={[remarkGfm, remarkWikilink]}
          components={components}
        >
          {source}
        </ReactMarkdown>
      </div>
    );
  } catch (e) {
    // Should be caught by ErrorBoundary in react-markdown 9.x, but just in case.
    return (
      <pre className="max-h-[60vh] overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800">
        {source}
      </pre>
    );
  }
}

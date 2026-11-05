import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useAppStore } from '@/store/useAppStore';
import { usePanelStore } from '@/store/panelStore';
import { useReadingPrefs, DEFAULT_READING_PREFS } from '@/store/readingPrefsStore';
import { useTranslation } from 'react-i18next';
import { changeLanguage } from '@/i18n';

interface AddState {
  name: string;
  path: string;
  error: string | null;
  isAdding: boolean;
}

const emptyAdd: AddState = { name: '', path: '', error: null, isAdding: false };

interface PrefSliderProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  display: string;
  onChange: (value: number) => void;
}

function PrefSlider({ label, value, min, max, step, display, onChange }: PrefSliderProps) {
  return (
    <div>
      <div className="flex items-center justify-between text-xs text-gray-600">
        <span>{label}</span>
        <span className="font-mono text-gray-500">{display}</span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full accent-blue-500"
      />
    </div>
  );
}

export function Settings() {
  const { t, i18n } = useTranslation('Settings');
  const currentLang = (i18n.language as 'zh' | 'en') ?? 'en';
  const kbs = useAppStore((s) => s.kbs);
  const activeKbId = useAppStore((s) => s.activeKbId);
  const activeKbEntryCount = useAppStore((s) => s.activeKbEntryCount);
  const refreshKbs = useAppStore((s) => s.refreshKbs);
  const loadEntries = useAppStore((s) => s.loadEntries);
  const addKb = useAppStore((s) => s.addKb);
  const removeKb = useAppStore((s) => s.removeKb);
  const setActiveKb = useAppStore((s) => s.setActiveKb);
  const templateHeader = useAppStore((s) => s.templateHeader);
  const saveTemplateHeader = useAppStore((s) => s.saveTemplateHeader);

  const readingPrefs = useReadingPrefs((s) => s.prefs);
  const updateReadingPrefs = useReadingPrefs((s) => s.update);
  const resetReadingPrefs = useReadingPrefs((s) => s.reset);

  const { close } = usePanelStore();

  const [headerDraft, setHeaderDraft] = useState(templateHeader);
  const [headerBusy, setHeaderBusy] = useState(false);
  const [headerNotice, setHeaderNotice] = useState<string | null>(null);
  const [headerError, setHeaderError] = useState<string | null>(null);

  // KB 配置异步载入后同步草稿；切换 KB 时也重新同步。
  useEffect(() => {
    setHeaderDraft(templateHeader);
    setHeaderNotice(null);
    setHeaderError(null);
  }, [templateHeader, activeKbId]);

  const handleSaveHeader = async () => {
    setHeaderBusy(true);
    setHeaderError(null);
    setHeaderNotice(null);
    try {
      await saveTemplateHeader(headerDraft);
      setHeaderNotice(t('template.saved'));
    } catch (e) {
      setHeaderError(t('template.saveFailed', { msg: (e as Error).message }));
    } finally {
      setHeaderBusy(false);
    }
  };

  const [add, setAdd] = useState<AddState>(emptyAdd);
  const [removeConfirmId, setRemoveConfirmId] = useState<string | null>(null);
  const [rebuilding, setRebuilding] = useState(false);
  const [globalError, setGlobalError] = useState<string | null>(null);

  const handleRebuild = async () => {
    setRebuilding(true);
    setGlobalError(null);
    try {
      const count = await invoke<number>('rebuild_active_kb');
      await loadEntries();
      setGlobalError(null);
      console.log(`Rebuilt ${count} entries`);
    } catch (e) {
      setGlobalError(t('errors.rebuildFailed', { msg: (e as Error).message }));
    } finally {
      setRebuilding(false);
    }
  };

  const handleRemove = async (id: string) => {
    setGlobalError(null);
    try {
      await removeKb(id);
      setRemoveConfirmId(null);
    } catch (e) {
      setGlobalError(t('errors.removeFailed', { msg: (e as Error).message }));
    }
  };

  const handleActivate = async (id: string) => {
    setGlobalError(null);
    try {
      await setActiveKb(id);
    } catch (e) {
      setGlobalError(t('errors.activateFailed', { msg: (e as Error).message }));
    }
  };

  const handleAdd = async () => {
    setAdd((s) => ({ ...s, isAdding: true, error: null }));
    try {
      await addKb(add.name.trim(), add.path.trim());
      setAdd(emptyAdd);
      await refreshKbs();
    } catch (e) {
      setAdd((s) => ({ ...s, error: t('errors.addFailed', { msg: (e as Error).message }) }));
    } finally {
      setAdd((s) => ({ ...s, isAdding: false }));
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-gray-200 p-4">
        <h2 className="text-lg font-semibold text-gray-900">{t('title')}</h2>
      </div>

      <div className="border-b border-gray-200 p-4">
        <div className="mb-2 text-sm font-medium text-gray-700">{t('language.label')}</div>
        <div className="flex gap-2">
          {(['zh', 'en'] as const).map((l) => (
            <button
              key={l}
              onClick={() => changeLanguage(l)}
              aria-pressed={currentLang === l}
              className={`rounded-md px-3 py-1.5 text-sm transition ${
                currentLang === l
                  ? 'bg-blue-500 text-white'
                  : 'border border-gray-300 bg-white text-gray-700 hover:bg-gray-50'
              }`}
            >
              {t(`language.option.${l}`)}
            </button>
          ))}
        </div>
      </div>

      {globalError && (
        <div className="border-b border-gray-200 bg-red-50 p-3 text-sm text-red-700">
          {globalError}
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-4">
        <section className="mb-4 border-b border-gray-200 pb-4">
          <div className="mb-1 text-sm font-medium text-gray-700">{t('template.label')}</div>
          <p className="mb-2 text-xs text-gray-500">{t('template.hint')}</p>
          <textarea
            rows={4}
            value={headerDraft}
            disabled={!activeKbId}
            onChange={(e) => {
              setHeaderDraft(e.target.value);
              setHeaderNotice(null);
            }}
            placeholder={t('template.placeholder')}
            className="w-full rounded-md border border-gray-300 px-3 py-2 font-mono text-xs focus:border-blue-500 focus:outline-none disabled:bg-gray-50"
          />
          <div className="mt-2 flex items-center gap-2">
            <button
              onClick={handleSaveHeader}
              disabled={!activeKbId || headerBusy}
              className="rounded-md bg-blue-500 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-600 disabled:opacity-50"
            >
              {headerBusy ? t('template.saving') : t('template.save')}
            </button>
            <button
              onClick={() => {
                setHeaderDraft('');
                setHeaderNotice(null);
              }}
              disabled={!activeKbId}
              className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50"
            >
              {t('template.clear')}
            </button>
            {headerNotice && <span className="text-xs text-green-600">{headerNotice}</span>}
          </div>
          {!activeKbId && <div className="mt-2 text-xs text-gray-500">{t('template.noKb')}</div>}
          {headerError && (
            <div className="mt-2 rounded-md bg-red-50 p-2 text-xs text-red-700">{headerError}</div>
          )}
        </section>

        <section className="mb-4 border-b border-gray-200 pb-4">
          <div className="mb-2 flex items-center justify-between">
            <div className="text-sm font-medium text-gray-700">{t('reading.label')}</div>
            <button
              onClick={resetReadingPrefs}
              className="rounded-md border border-gray-300 bg-white px-2 py-1 text-xs text-gray-700 hover:bg-gray-50"
            >
              {t('reading.reset')}
            </button>
          </div>
          <div className="space-y-2.5">
            <PrefSlider
              label={t('reading.fontSize')}
              value={readingPrefs.fontSize}
              min={13}
              max={20}
              step={1}
              display={`${readingPrefs.fontSize}px`}
              onChange={(fontSize) => updateReadingPrefs({ fontSize })}
            />
            <PrefSlider
              label={t('reading.lineHeight')}
              value={readingPrefs.lineHeight}
              min={1.4}
              max={2.2}
              step={0.05}
              display={readingPrefs.lineHeight.toFixed(2)}
              onChange={(lineHeight) => updateReadingPrefs({ lineHeight })}
            />
            <PrefSlider
              label={t('reading.paragraphGap')}
              value={readingPrefs.paragraphGap}
              min={0.2}
              max={1.4}
              step={0.05}
              display={`${readingPrefs.paragraphGap.toFixed(2)}em`}
              onChange={(paragraphGap) => updateReadingPrefs({ paragraphGap })}
            />
            <PrefSlider
              label={t('reading.maxWidth')}
              value={readingPrefs.maxWidth}
              min={0}
              max={110}
              step={1}
              display={readingPrefs.maxWidth === 0 ? t('reading.unlimited') : `${readingPrefs.maxWidth}ch`}
              onChange={(maxWidth) => updateReadingPrefs({ maxWidth })}
            />
            <PrefSlider
              label={t('reading.codeFontSize')}
              value={readingPrefs.codeFontSize}
              min={11}
              max={16}
              step={0.5}
              display={`${readingPrefs.codeFontSize}px`}
              onChange={(codeFontSize) => updateReadingPrefs({ codeFontSize })}
            />
            <div>
              <div className="mb-1 text-xs text-gray-600">{t('reading.tableStyle')}</div>
              <div className="flex gap-2">
                {(['bordered', 'striped'] as const).map((style) => (
                  <button
                    key={style}
                    onClick={() => updateReadingPrefs({ tableStyle: style })}
                    aria-pressed={readingPrefs.tableStyle === style}
                    className={`rounded-md px-2.5 py-1 text-xs transition ${
                      readingPrefs.tableStyle === style
                        ? 'bg-blue-500 text-white'
                        : 'border border-gray-300 bg-white text-gray-700 hover:bg-gray-50'
                    }`}
                  >
                    {t(`reading.table.${style}`)}
                  </button>
                ))}
              </div>
            </div>
          </div>
          <p className="mt-2 text-xs text-gray-500">
            {t('reading.hint', { defaults: `${DEFAULT_READING_PREFS.fontSize}px / ${DEFAULT_READING_PREFS.lineHeight}` })}
          </p>
        </section>

        {kbs.length === 0 ? (
          <div className="rounded-md border border-dashed border-gray-300 p-6 text-center text-sm text-gray-500">
            {t('empty')}
          </div>
        ) : (
          <ul className="space-y-2">
            {kbs.map((kb) => {
              const isActive = kb.id === activeKbId;
              const showCount = isActive;
              const isConfirmingRemove = removeConfirmId === kb.id;
              return (
                <li
                  key={kb.id}
                  className={`rounded-md border p-3 transition ${
                    isActive
                      ? 'border-blue-300 bg-blue-50/40'
                      : 'border-gray-200 bg-white'
                  }`}
                >
                  <div className="flex items-start gap-3">
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-sm font-medium text-gray-900">
                          {kb.name}
                        </span>
                        {isActive && (
                          <span className="rounded-full bg-blue-100 px-2 py-0.5 text-xs font-medium text-blue-700">
                            {t('activeBadge')}
                          </span>
                        )}
                        {showCount && (
                          <span className="text-xs text-gray-400">
                            {activeKbEntryCount}{' '}
                            {activeKbEntryCount === 1 ? t('entryCountOne') : t('entryCountMany')}
                          </span>
                        )}
                      </div>
                      <div className="mt-0.5 truncate font-mono text-xs text-gray-500">
                        {kb.path}
                      </div>
                    </div>
                    <div className="flex shrink-0 items-center gap-1">
                      {isActive && (
                        <button
                          onClick={handleRebuild}
                          disabled={rebuilding}
                          className="rounded-md border border-gray-300 bg-white px-2 py-1 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50"
                          title={t('rebuild')}
                        >
                          {rebuilding ? t('rebuilding') : t('rebuild')}
                        </button>
                      )}
                      {!isActive && (
                        <button
                          onClick={() => handleActivate(kb.id)}
                          className="rounded-md border border-gray-300 bg-white px-2 py-1 text-xs text-gray-700 hover:bg-gray-50"
                        >
                          {t('activate')}
                        </button>
                      )}
                      {isConfirmingRemove ? (
                        <>
                          <button
                            onClick={() => handleRemove(kb.id)}
                            className="rounded-md bg-red-500 px-2 py-1 text-xs font-medium text-white hover:bg-red-600"
                          >
                            {t('confirmRemove')}
                          </button>
                          <button
                            onClick={() => setRemoveConfirmId(null)}
                            className="rounded-md border border-gray-300 bg-white px-2 py-1 text-xs text-gray-700 hover:bg-gray-50"
                          >
                            {t('cancelRemove')}
                          </button>
                        </>
                      ) : (
                        <button
                          onClick={() => setRemoveConfirmId(kb.id)}
                          className="rounded-md border border-gray-300 bg-white px-2 py-1 text-xs text-red-600 hover:bg-red-50"
                          title={t('removeTitle')}
                        >
                          {t('remove')}
                        </button>
                      )}
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <div className="border-t border-gray-200 p-4">
        <div className="mb-2 text-sm font-medium text-gray-700">{t('addLabel')}</div>
        <div className="flex flex-col gap-2">
          <input
            type="text"
            value={add.name}
            onChange={(e) => setAdd({ ...add, name: e.target.value })}
            placeholder={t('namePlaceholder')}
            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none"
          />
          <div className="flex gap-2">
            <input
              type="text"
              value={add.path}
              onChange={(e) => setAdd({ ...add, path: e.target.value })}
              placeholder={t('pathPlaceholder')}
              className="flex-1 rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none"
            />
            <button
              onClick={async () => {
                try {
                  const chosen = await open({
                    directory: true,
                    multiple: false,
                    title: t('browseTitle'),
                  });
                  if (typeof chosen === 'string') {
                    setAdd((s) => ({ ...s, path: chosen }));
                  }
                } catch (e) {
                  setAdd((s) => ({ ...s, error: t('errors.addFailed', { msg: (e as Error).message }) }));
                }
              }}
              className="rounded-md border border-gray-300 bg-white px-3 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50"
            >
              {t('browse')}
            </button>
          </div>
          {add.error && (
            <div className="rounded-md bg-red-50 p-2 text-sm text-red-700">
              {add.error}
            </div>
          )}
          <button
            onClick={handleAdd}
            disabled={!add.name.trim() || !add.path.trim() || add.isAdding}
            className="self-end rounded-md bg-blue-500 px-4 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {add.isAdding ? t('addBusy') : t('addIdle')}
          </button>
        </div>
      </div>
    </div>
  );
}

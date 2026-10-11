import { i18n } from './index';

/// Structured error payload from the Rust backend. After Finding #14,
/// every `AppError` variant serializes as `{ code: "kebab-case", ...params }`
/// so the frontend can localize by code instead of receiving raw
/// hardcoded strings. The `code` is the key into the `Errors` namespace.
export interface AppErrorPayload {
  code: string;
  path?: string;
  kbId?: string;
  id?: string;
  name?: string;
  parentGroup?: string;
  message?: string;
  [k: string]: unknown;
}

/// Type guard: an Error thrown by `invoke()` may be a plain JS Error
/// (e.g. network/serialization failure) or an `AppErrorPayload` JSON.
/// Returns null for plain errors — those keep showing as `.message`.
function asPayload(e: unknown): AppErrorPayload | null {
  if (e && typeof e === 'object' && 'code' in e && typeof (e as { code: unknown }).code === 'string') {
    return e as AppErrorPayload;
  }
  return null;
}

/// Map an error code to a translated string using the current i18next
/// language. Falls back to the original error message when the code is
/// unknown so untranslated errors still surface something useful. The
/// param names match the kebab-case keys in `Errors.{zh,en}.json`.
export function localizeError(e: unknown): string {
  const payload = asPayload(e);
  if (!payload) {
    return e instanceof Error ? e.message : String(e);
  }
  const { code, ...params } = payload;
  // CamelCase the param names so they read naturally in templates
  // (e.g. {{kbId}} instead of {{kb_id}}).
  const camelParams: Record<string, string> = {};
  for (const [k, v] of Object.entries(params)) {
    const camel = k.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
    camelParams[camel] = String(v);
  }
  const translated = i18n.t(`errors.${code}`, {
    ns: 'Errors',
    ...camelParams,
    defaultValue: payload.message ?? '',
  });
  if (translated && translated !== code) return translated;
  // Last-resort fallback so the user always sees something.
  return i18n.t('errors.fallback', {
    ns: 'Errors',
    message: payload.message ?? code,
    defaultValue: payload.message ?? code,
  });
}
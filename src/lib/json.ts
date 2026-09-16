/** JSON helpers for the message and configuration editors. */

export function tryParseJson(value: string): { ok: true; value: unknown } | { ok: false; error: string } {
  try {
    return { ok: true, value: JSON.parse(value) };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'Invalid JSON' };
  }
}

/**
 * Pretty-print a payload when it looks like JSON, otherwise return it untouched.
 * Never throws — message payloads are arbitrary bytes.
 */
export function prettify(value: string | null | undefined, enabled = true): string {
  if (!value) return '';
  if (!enabled) return value;
  const trimmed = value.trim();
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return value;
  const parsed = tryParseJson(trimmed);
  if (!parsed.ok) return value;
  try {
    return JSON.stringify(parsed.value, null, 2);
  } catch {
    return value;
  }
}

/** One-line preview used in the message table. */
export function compact(value: string | null | undefined, max = 160): string {
  if (!value) return '';
  const single = value.replace(/\s+/g, ' ').trim();
  return single.length > max ? `${single.slice(0, max - 1)}…` : single;
}

export function isJsonObject(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return false;
  return tryParseJson(trimmed).ok;
}

/** Convert the key/value editor rows into a plain object. */
export function rowsToObject(rows: Array<{ key: string; value: string }>): Record<string, string> {
  const output: Record<string, string> = {};
  for (const row of rows) {
    const key = row.key.trim();
    if (key) output[key] = row.value;
  }
  return output;
}

export function objectToRows(value: unknown): Array<{ key: string; value: string }> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return [];
  return Object.entries(value as Record<string, unknown>).map(([key, raw]) => ({
    key,
    value: typeof raw === 'string' ? raw : JSON.stringify(raw),
  }));
}

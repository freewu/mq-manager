import { useMemo } from 'react';

import { CopyButton } from './primitives';
import { byteLength } from '@/lib/format';
import { tryParseJson } from '@/lib/json';
import { cn } from '@/lib/cn';

/**
 * Deliberately a plain textarea: no Monaco / CodeMirror in the bundle.
 * Validation runs on every keystroke and is reported under the editor.
 */
export function JsonEditor({
  value,
  onChange,
  rows = 10,
  placeholder = '{\n  "hello": "world"\n}',
  encoding = 'utf8',
  allowEmpty = true,
  label = 'JSON',
}: {
  value: string;
  onChange: (value: string) => void;
  rows?: number;
  placeholder?: string;
  encoding?: 'utf8' | 'base64';
  allowEmpty?: boolean;
  label?: string;
}) {
  const validity = useMemo(() => {
    if (encoding === 'base64') return { kind: 'ok' as const, text: 'base64 payload' };
    if (value.trim() === '') {
      return allowEmpty
        ? ({ kind: 'ok' as const, text: 'empty' })
        : ({ kind: 'empty' as const, text: 'required' });
    }
    const result = tryParseJson(value);
    return result.ok ? { kind: 'ok' as const, text: 'valid JSON' } : { kind: 'error' as const, text: result.error };
  }, [value, encoding, allowEmpty]);

  const format = () => {
    const result = tryParseJson(value);
    if (!result.ok) return;
    onChange(JSON.stringify(result.value, null, 2));
  };

  return (
    <div className="json-editor">
      <textarea
        className={cn('textarea textarea--mono', validity.kind === 'error' && 'textarea--invalid')}
        style={{ minHeight: rows * 19 + 16 }}
        spellCheck={false}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
      />
      <div className="json-editor__meta">
        <span
          className={cn(
            validity.kind === 'error' && 'text-danger',
            validity.kind === 'ok' && 'dim',
          )}
        >
          {label}: {validity.text}
        </span>
        <span className="row gap-6">
          <span className="dim tiny mono">
            {byteLength(value)} B
          </span>
          {encoding === 'utf8' ? (
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={format}
              disabled={validity.kind !== 'ok' || value.trim() === ''}
            >
              Format
            </button>
          ) : null}
          <CopyButton value={value} />
        </span>
      </div>
    </div>
  );
}

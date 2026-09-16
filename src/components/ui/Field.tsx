import type { ReactNode } from 'react';

import { Icon } from './Icon';
import { cn } from '@/lib/cn';

export function Field({
  label,
  help,
  error,
  required = false,
  htmlFor,
  children,
  full = false,
  action,
}: {
  label?: ReactNode;
  help?: ReactNode;
  error?: ReactNode;
  required?: boolean;
  htmlFor?: string;
  children: ReactNode;
  /** Span every column of a `.form-grid`. */
  full?: boolean;
  action?: ReactNode;
}) {
  return (
    <div className={cn('field', full && 'form-grid--full')}>
      {label ? (
        <label className="field__label" htmlFor={htmlFor}>
          <span>
            {label}
            {required ? <span className="req"> *</span> : null}
          </span>
          {action}
        </label>
      ) : null}
      {children}
      {error ? <span className="field__error">{error}</span> : null}
      {!error && help ? <span className="field__help">{help}</span> : null}
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  disabled = false,
  help,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: ReactNode;
  disabled?: boolean;
  help?: ReactNode;
}) {
  return (
    <div className="stack-4">
      <label className="checkbox" style={{ opacity: disabled ? 0.5 : 1 }}>
        <input
          type="checkbox"
          checked={checked}
          disabled={disabled}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span>{label}</span>
      </label>
      {help ? <span className="field__help">{help}</span> : null}
    </div>
  );
}

export function Checkbox({
  checked,
  onChange,
  label,
  disabled = false,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: ReactNode;
  disabled?: boolean;
}) {
  return (
    <label className="checkbox" style={{ opacity: disabled ? 0.5 : 1 }}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span>{label}</span>
    </label>
  );
}

export interface Option {
  value: string;
  label: string;
  disabled?: boolean;
}

export function Select({
  value,
  onChange,
  options,
  disabled = false,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  options: Option[];
  disabled?: boolean;
  className?: string;
}) {
  return (
    <select
      className={cn('select', className)}
      value={value}
      disabled={disabled}
      onChange={(event) => onChange(event.target.value)}
    >
      {options.map((option) => (
        <option key={option.value} value={option.value} disabled={option.disabled}>
          {option.label}
        </option>
      ))}
    </select>
  );
}

/* -------------------------------------------------------------------------- */
/* Key/value editor (headers, options, configs)                               */
/* -------------------------------------------------------------------------- */

export interface KeyValueRow {
  key: string;
  value: string;
}

export function KeyValueEditor({
  rows,
  onChange,
  keyPlaceholder = 'key',
  valuePlaceholder = 'value',
  addLabel = 'Add row',
  emptyLabel = 'No entries',
}: {
  rows: KeyValueRow[];
  onChange: (rows: KeyValueRow[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  addLabel?: string;
  emptyLabel?: string;
}) {
  const update = (index: number, patch: Partial<KeyValueRow>) => {
    onChange(rows.map((row, position) => (position === index ? { ...row, ...patch } : row)));
  };

  return (
    <div className="kv">
      {rows.length === 0 ? <span className="kv__empty">{emptyLabel}</span> : null}
      {rows.map((row, index) => (
        <div className="kv__row" key={index}>
          <input
            className="input input--mono"
            value={row.key}
            placeholder={keyPlaceholder}
            spellCheck={false}
            onChange={(event) => update(index, { key: event.target.value })}
          />
          <input
            className="input input--mono"
            value={row.value}
            placeholder={valuePlaceholder}
            spellCheck={false}
            onChange={(event) => update(index, { value: event.target.value })}
          />
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Remove row"
            onClick={() => onChange(rows.filter((_, position) => position !== index))}
          >
            <Icon name="x" size={12} />
          </button>
        </div>
      ))}
      <div>
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          onClick={() => onChange([...rows, { key: '', value: '' }])}
        >
          <Icon name="plus" size={12} />
          {addLabel}
        </button>
      </div>
    </div>
  );
}

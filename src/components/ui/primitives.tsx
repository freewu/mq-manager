import {
  forwardRef,
  useState,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type ReactNode,
} from 'react';

import { Icon, type IconName } from './Icon';
import { cn } from '@/lib/cn';
import { copyText } from '@/lib/clipboard';
import type { ConnectionState } from '@/types';

/* -------------------------------------------------------------------------- */
/* Buttons                                                                    */
/* -------------------------------------------------------------------------- */

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'default' | 'primary' | 'danger' | 'success' | 'ghost' | 'outline';
  size?: 'sm' | 'md';
  icon?: IconName;
  iconRight?: IconName;
  loading?: boolean;
  block?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    variant = 'default',
    size = 'md',
    icon,
    iconRight,
    loading = false,
    block = false,
    className,
    children,
    disabled,
    ...rest
  },
  ref,
) {
  return (
    <button
      ref={ref}
      type="button"
      className={cn(
        'btn',
        variant !== 'default' && `btn--${variant}`,
        size === 'sm' && 'btn--sm',
        block && 'btn--block',
        className,
      )}
      disabled={disabled || loading}
      {...rest}
    >
      {loading ? <span className="spinner" /> : icon ? <Icon name={icon} /> : null}
      {children}
      {iconRight ? <Icon name={iconRight} /> : null}
    </button>
  );
});

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: IconName;
  label: string;
  variant?: 'default' | 'ghost' | 'danger';
  size?: 'sm' | 'md';
}

export function IconButton({
  icon,
  label,
  variant = 'ghost',
  size = 'md',
  className,
  ...rest
}: IconButtonProps) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      className={cn(
        'btn',
        'btn--icon',
        variant !== 'default' && `btn--${variant}`,
        size === 'sm' && 'btn--sm',
        className,
      )}
      {...rest}
    >
      <Icon name={icon} />
    </button>
  );
}

/* -------------------------------------------------------------------------- */
/* Badges, dots, chips                                                        */
/* -------------------------------------------------------------------------- */

export type BadgeTone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info';

export function Badge({
  tone = 'neutral',
  mono = false,
  outline = false,
  icon,
  children,
  title,
}: {
  tone?: BadgeTone;
  mono?: boolean;
  outline?: boolean;
  icon?: IconName;
  children: ReactNode;
  title?: string;
}) {
  return (
    <span
      title={title}
      className={cn(
        'badge',
        tone !== 'neutral' && `badge--${tone}`,
        mono && 'badge--mono',
        outline && 'badge--outline',
      )}
    >
      {icon ? <Icon name={icon} size={11} /> : null}
      {children}
    </span>
  );
}

export function Tag({ children, title }: { children: ReactNode; title?: string }) {
  return (
    <span className="tag" title={title}>
      {children}
    </span>
  );
}

const STATE_TONE: Record<ConnectionState, BadgeTone> = {
  connected: 'success',
  connecting: 'warning',
  error: 'danger',
  disconnected: 'neutral',
};

export function stateTone(state: ConnectionState): BadgeTone {
  return STATE_TONE[state];
}

export function StatusDot({ state, title }: { state: ConnectionState; title?: string }) {
  return <span className={cn('dot', `dot--${state}`)} title={title ?? state} />;
}

/** Small coloured pill showing which engine a connection speaks to. */
export function ProviderChip({
  name,
  accent,
  glyph,
  compact = false,
}: {
  name: string;
  accent?: string;
  glyph?: string;
  compact?: boolean;
}) {
  const letter = (glyph ?? name.slice(0, 1)).toUpperCase();
  return (
    <span className="provider-chip" style={{ color: accent ?? 'var(--accent)' }} title={name}>
      <span className="provider-chip__glyph">{letter}</span>
      {compact ? null : name}
    </span>
  );
}

export function Spinner({ large = false }: { large?: boolean }) {
  return <span className={cn('spinner', large && 'spinner--lg')} />;
}

/* -------------------------------------------------------------------------- */
/* Surfaces                                                                   */
/* -------------------------------------------------------------------------- */

export function Panel({
  title,
  actions,
  footer,
  children,
  className,
  bodyClassName,
  flush = false,
}: {
  title?: ReactNode;
  actions?: ReactNode;
  footer?: ReactNode;
  children: ReactNode;
  className?: string;
  bodyClassName?: string;
  flush?: boolean;
}) {
  return (
    <section className={cn('panel', className)}>
      {title || actions ? (
        <header className="panel__head">
          <div className="panel__title grow ellipsis">{title}</div>
          {actions ? <div className="row gap-6">{actions}</div> : null}
        </header>
      ) : null}
      <div className={cn(flush ? 'panel__body--tight' : 'panel__body', bodyClassName)}>{children}</div>
      {footer ? <footer className="panel__foot">{footer}</footer> : null}
    </section>
  );
}

export function Metric({
  label,
  value,
  sub,
  icon,
}: {
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  icon?: IconName;
}) {
  return (
    <div className="metric">
      <div className="row-between">
        <span className="metric__label">{label}</span>
        {icon ? <Icon name={icon} size={13} className="dim" /> : null}
      </div>
      <span className="metric__value">{value}</span>
      {sub ? <span className="metric__sub">{sub}</span> : null}
    </div>
  );
}

export function Banner({
  tone = 'info',
  icon,
  children,
  actions,
}: {
  tone?: 'info' | 'warning' | 'danger';
  icon?: IconName;
  children: ReactNode;
  actions?: ReactNode;
}) {
  const fallback: IconName = tone === 'info' ? 'info' : 'alert';
  return (
    <div className={cn('banner', `banner--${tone}`)}>
      <Icon name={icon ?? fallback} size={15} />
      <div className="banner__text">{children}</div>
      {actions}
    </div>
  );
}

export function EmptyState({
  icon = 'inbox',
  title,
  text,
  actions,
}: {
  icon?: IconName;
  title: string;
  text?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="empty">
      <span className="empty__icon">
        <Icon name={icon} size={20} />
      </span>
      <span className="empty__title">{title}</span>
      {text ? <p className="empty__text">{text}</p> : null}
      {actions ? <div className="row gap-8 mt-8">{actions}</div> : null}
    </div>
  );
}

export function Progress({ value, max = 100 }: { value: number; max?: number }) {
  const percent = max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0;
  return (
    <div className="progress">
      <div className="progress__fill" style={{ width: `${percent}%` }} />
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Toolbar + search                                                           */
/* -------------------------------------------------------------------------- */

export function Toolbar({ children, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className="toolbar" {...rest}>
      {children}
    </div>
  );
}

export function ToolbarSpacer() {
  return <span className="toolbar__spacer" />;
}

export function SearchInput({
  value,
  onChange,
  placeholder = 'Search…',
  width = 240,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  width?: number;
}) {
  return (
    <span className="search-wrap" style={{ width }}>
      <Icon name="search" />
      <input
        className="input input--search"
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        spellCheck={false}
      />
      {value ? (
        <button
          type="button"
          className="btn btn--ghost btn--icon btn--sm"
          style={{ position: 'absolute', right: 3 }}
          onClick={() => onChange('')}
          aria-label="Clear search"
        >
          <Icon name="x" size={12} />
        </button>
      ) : null}
    </span>
  );
}

/* -------------------------------------------------------------------------- */
/* Copy + details                                                             */
/* -------------------------------------------------------------------------- */

export function CopyButton({
  value,
  label = 'Copy',
  size = 'sm',
}: {
  value: string;
  label?: string;
  size?: 'sm' | 'md';
}) {
  const [copied, setCopied] = useState(false);

  return (
    <Button
      size={size}
      variant="ghost"
      icon={copied ? 'check' : 'copy'}
      onClick={async () => {
        const ok = await copyText(value);
        if (!ok) return;
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1400);
      }}
      title={label}
    >
      {size === 'md' ? (copied ? 'Copied' : label) : null}
    </Button>
  );
}

export function DetailList({ items }: { items: Array<{ label: string; value: ReactNode }> }) {
  return (
    <dl className="detail-list">
      {items.map((item, index) => (
        <div key={`${item.label}-${index}`} style={{ display: 'contents' }}>
          <dt>{item.label}</dt>
          <dd>{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}

export function SectionTitle({ children, actions }: { children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="row-between" style={{ marginBottom: 10 }}>
      <h2>{children}</h2>
      {actions}
    </div>
  );
}

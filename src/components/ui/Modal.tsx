import { useEffect, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

import { Button, type ButtonProps } from './primitives';
import { Icon } from './Icon';
import { cn } from '@/lib/cn';

export interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  subtitle?: ReactNode;
  /** `center` for forms, `right` for detail drawers. */
  variant?: 'center' | 'right';
  size?: 'sm' | 'md' | 'lg' | 'xl';
  children: ReactNode;
  footer?: ReactNode;
  /** Disable closing on scrim click / Escape (e.g. while a request is in flight). */
  locked?: boolean;
}

export function Modal({
  open,
  onClose,
  title,
  subtitle,
  variant = 'center',
  size = 'md',
  children,
  footer,
  locked = false,
}: ModalProps) {
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !locked) onClose();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open, locked, onClose]);

  if (!open) return null;

  return createPortal(
    <div
      className={cn('scrim', variant === 'right' ? 'scrim--right' : 'scrim--center')}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !locked) onClose();
      }}
    >
      <div
        className={cn(
          'modal',
          variant === 'right' ? 'modal--drawer' : `modal--${size}`,
        )}
        role="dialog"
        aria-modal="true"
      >
        <header className="modal__head">
          <div className="grow" style={{ minWidth: 0 }}>
            <div className="modal__title ellipsis">{title}</div>
            {subtitle ? <div className="small dim ellipsis">{subtitle}</div> : null}
          </div>
          <Button variant="ghost" size="sm" icon="x" onClick={onClose} aria-label="Close" />
        </header>
        <div className="modal__body">{children}</div>
        {footer ? <footer className="modal__foot">{footer}</footer> : null}
      </div>
    </div>,
    document.body,
  );
}

export interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: ButtonProps['variant'];
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  tone = 'danger',
  busy = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  return (
    <Modal
      open={open}
      onClose={onCancel}
      title={title}
      size="sm"
      locked={busy}
      footer={
        <>
          <Button variant="ghost" onClick={onCancel} disabled={busy}>
            {cancelLabel}
          </Button>
          <Button variant={tone} onClick={onConfirm} loading={busy}>
            {confirmLabel}
          </Button>
        </>
      }
    >
      <div className="row gap-12" style={{ alignItems: 'flex-start' }}>
        <span className={tone === 'danger' ? 'text-danger' : 'text-warning'} style={{ marginTop: 2 }}>
          <Icon name="alert" size={18} />
        </span>
        <div className="grow small muted" data-selectable>
          {message}
        </div>
      </div>
    </Modal>
  );
}

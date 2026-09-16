import { Icon, type IconName } from './Icon';
import { useToasts, type ToastKind } from '@/store/toast';

const ICONS: Record<ToastKind, IconName> = {
  success: 'check',
  error: 'alert',
  warning: 'alert',
  info: 'info',
};

export function ToastHost() {
  const toasts = useToasts((state) => state.toasts);
  const dismiss = useToasts((state) => state.dismiss);

  if (toasts.length === 0) return null;

  return (
    <div className="toasts">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast toast--${toast.kind}`} role="status">
          <span className="toast__icon">
            <Icon name={ICONS[toast.kind]} size={14} />
          </span>
          <div className="grow">
            <div className="toast__title">{toast.title}</div>
            {toast.message ? <div className="toast__msg">{toast.message}</div> : null}
          </div>
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Dismiss"
            onClick={() => dismiss(toast.id)}
          >
            <Icon name="x" size={12} />
          </button>
        </div>
      ))}
    </div>
  );
}

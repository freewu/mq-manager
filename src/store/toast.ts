import { create } from 'zustand';

import { errorMessage } from '@/api/client';

export type ToastKind = 'success' | 'error' | 'info' | 'warning';

export interface Toast {
  id: string;
  kind: ToastKind;
  title: string;
  message?: string;
}

interface ToastState {
  toasts: Toast[];
  push: (toast: Omit<Toast, 'id'> & { id?: string }) => string;
  dismiss: (id: string) => void;
  clear: () => void;
}

const LIFETIME: Record<ToastKind, number> = {
  success: 3500,
  info: 4000,
  warning: 6000,
  error: 9000,
};

let counter = 0;

export const useToasts = create<ToastState>((set) => ({
  toasts: [],

  push(toast) {
    counter += 1;
    const id = toast.id ?? `toast-${counter}`;
    set((state) => ({ toasts: [...state.toasts, { ...toast, id }] }));
    window.setTimeout(() => {
      set((state) => ({ toasts: state.toasts.filter((entry) => entry.id !== id) }));
    }, LIFETIME[toast.kind]);
    return id;
  },

  dismiss(id) {
    set((state) => ({ toasts: state.toasts.filter((entry) => entry.id !== id) }));
  },

  clear() {
    set({ toasts: [] });
  },
}));

export const notify = (kind: ToastKind, title: string, message?: string) =>
  useToasts.getState().push({ kind, title, message });

export const notifySuccess = (title: string, message?: string) => notify('success', title, message);

export const notifyInfo = (title: string, message?: string) => notify('info', title, message);

/** Renders any thrown value as an error toast and returns the message. */
export function notifyError(error: unknown, title = 'Operation failed'): string {
  const message = errorMessage(error);
  notify('error', title, message);
  return message;
}

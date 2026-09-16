import { invoke } from '@tauri-apps/api/core';

import type { ApiErrorPayload } from '@/types';

/** Normalised backend error. `kind` lets callers react to specific failures. */
export class ApiError extends Error {
  readonly kind: string;

  constructor(kind: string, message: string) {
    super(message);
    this.name = 'ApiError';
    this.kind = kind;
  }

  get unsupported(): boolean {
    return this.kind === 'unsupported';
  }

  get notConnected(): boolean {
    return this.kind === 'connectionNotFound';
  }
}

export function isApiError(error: unknown): error is ApiError {
  return error instanceof ApiError;
}

export function errorMessage(error: unknown): string {
  if (isApiError(error)) return error.message;
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  return 'Something went wrong';
}

/** `true` when the UI runs inside the Tauri webview. */
export function isDesktop(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

function normalise(raw: unknown): ApiError {
  if (isApiError(raw)) return raw;
  if (raw && typeof raw === 'object' && 'kind' in raw && 'message' in raw) {
    const payload = raw as ApiErrorPayload;
    return new ApiError(String(payload.kind), String(payload.message));
  }
  return new ApiError('unknown', typeof raw === 'string' ? raw : JSON.stringify(raw));
}

/**
 * Thin wrapper around `invoke` that always rejects with an {@link ApiError}.
 */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isDesktop()) {
    throw new ApiError(
      'noTauri',
      'The backend is not reachable. Run `just dev` (or `pnpm tauri dev`) instead of opening the dev server in a browser.',
    );
  }
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    throw normalise(raw);
  }
}

/** Fire-and-forget variant used for non-critical refreshes. */
export function swallow(promise: Promise<unknown>): void {
  void promise.catch(() => undefined);
}

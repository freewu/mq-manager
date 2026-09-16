import { useCallback, useEffect, useRef, useState } from 'react';

import { errorMessage } from '@/api/client';

export interface AsyncState<T> {
  data: T | undefined;
  loading: boolean;
  error: string | undefined;
}

export interface AsyncResult<T> extends AsyncState<T> {
  reload: () => Promise<T | undefined>;
  setData: (data: T | undefined) => void;
}

/**
 * Runs an async loader whenever `deps` change and tracks loading / error state.
 * The loader identity is not part of `deps`, so inline arrow functions are fine.
 */
export function useAsync<T>(
  loader: () => Promise<T>,
  deps: unknown[],
  options: { enabled?: boolean } = {},
): AsyncResult<T> {
  const enabled = options.enabled ?? true;
  const [state, setState] = useState<AsyncState<T>>({
    data: undefined,
    loading: enabled,
    error: undefined,
  });
  const mounted = useRef(true);
  const loaderRef = useRef(loader);
  loaderRef.current = loader;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const reload = useCallback(async () => {
    setState((previous) => ({ ...previous, loading: true, error: undefined }));
    try {
      const data = await loaderRef.current();
      if (mounted.current) setState({ data, loading: false, error: undefined });
      return data;
    } catch (error) {
      if (mounted.current) {
        setState((previous) => ({ ...previous, loading: false, error: errorMessage(error) }));
      }
      return undefined;
    }
  }, []);

  useEffect(() => {
    if (!enabled) {
      setState({ data: undefined, loading: false, error: undefined });
      return;
    }
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, reload, ...deps]);

  const setData = useCallback((data: T | undefined) => {
    setState((previous) => ({ ...previous, data }));
  }, []);

  return { ...state, reload, setData };
}

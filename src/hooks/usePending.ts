import { useCallback, useEffect, useRef, useState } from 'react';

/** `true` while the promise returned by the wrapped callback is pending. */
export function usePending(): [boolean, <T>(task: () => Promise<T>) => Promise<T | undefined>] {
  const [pending, setPending] = useState(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const run = useCallback(async <T,>(task: () => Promise<T>): Promise<T | undefined> => {
    setPending(true);
    try {
      return await task();
    } catch {
      return undefined;
    } finally {
      if (mounted.current) setPending(false);
    }
  }, []);

  return [pending, run];
}

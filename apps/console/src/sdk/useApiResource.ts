import { useCallback, useEffect, useRef, useState } from 'react';

export interface ApiResource<T> {
  data: T | null;
  error: unknown;
  loading: boolean;
  reload: () => void;
}

/**
 * Loads a resource once on mount and on explicit `reload()`.
 *
 * `load` is held in a ref rather than used as an effect dependency, so callers
 * may pass an inline lambda without triggering a fetch loop.
 */
export function useApiResource<T>(load: () => Promise<T>): ApiResource<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);

  const loadRef = useRef(load);
  loadRef.current = load;

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(null);

    loadRef
      .current()
      .then((value) => {
        if (active) setData(value);
      })
      .catch((reason: unknown) => {
        if (active) {
          setData(null);
          setError(reason ?? new Error('API request failed'));
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [attempt]);

  const reload = useCallback(() => setAttempt((value) => value + 1), []);

  return { data, error, loading, reload };
}

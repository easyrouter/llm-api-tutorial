/**
 * `useAsync` — tiny state machine around a promise-returning function (typically one of the
 * typed `invoke` wrappers in `lib/tauri.ts`).
 *
 * ```tsx
 * const checks = useAsync(runEnvChecks);
 * useEffect(() => { void checks.run(); }, [checks.run]);
 * if (checks.loading) return <Spinner />;
 * if (checks.error) return <ErrorBanner error={checks.error} onRetry={() => void checks.run()} />;
 * ```
 *
 * - `run` is referentially stable; the latest `fn` is always used (pass inline lambdas freely).
 * - Results that arrive after the component unmounted, after `reset()`, or after a newer `run`
 *   started are ignored — no "setState on unmounted component", no stale data winning a race.
 * - Errors are normalised to `WireError` (see `lib/errors.ts`) so screens can pass them straight
 *   to `ErrorBanner` / `describeError`. `run` never throws; it resolves to `undefined` on error.
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import { toWireError } from "./errors";
import type { WireError } from "./types";

export interface AsyncState<T> {
  /** Last successful result; kept while a re-run is in flight so the UI does not flicker. */
  data: T | null;
  /** Error of the last run, cleared when a new run starts or on `reset()`. */
  error: WireError | null;
  /** True while the latest run is pending. */
  loading: boolean;
}

export interface UseAsyncResult<A extends unknown[], T> extends AsyncState<T> {
  /** Starts a run. Resolves with the value on success, `undefined` on error/cancellation. */
  run: (...args: A) => Promise<T | undefined>;
  /** Clears data/error/loading and invalidates any in-flight run. */
  reset: () => void;
}

export interface UseAsyncOptions<T> {
  onSuccess?: (data: T) => void;
  onError?: (error: WireError) => void;
}

const idle = { data: null, error: null, loading: false };

export function useAsync<A extends unknown[], T>(
  fn: (...args: A) => Promise<T>,
  options: UseAsyncOptions<T> = {},
): UseAsyncResult<A, T> {
  const [state, setState] = useState<AsyncState<T>>(idle);
  const fnRef = useRef(fn);
  const optionsRef = useRef(options);
  const runIdRef = useRef(0);
  const mountedRef = useRef(true);

  // Keep the latest callbacks without making `run` depend on them. Layout effects run before
  // any passive effect (in children too), so `run` invoked from a mount effect sees fresh values.
  useLayoutEffect(() => {
    fnRef.current = fn;
    optionsRef.current = options;
  });

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      runIdRef.current += 1;
    };
  }, []);

  const run = useCallback(async (...args: A): Promise<T | undefined> => {
    const id = ++runIdRef.current;
    const isCurrent = () => mountedRef.current && runIdRef.current === id;
    setState((s) => ({ ...s, error: null, loading: true }));
    try {
      const data = await fnRef.current(...args);
      if (!isCurrent()) return undefined;
      setState({ data, error: null, loading: false });
      optionsRef.current.onSuccess?.(data);
      return data;
    } catch (e) {
      if (!isCurrent()) return undefined;
      const error = toWireError(e);
      setState((s) => ({ ...s, error, loading: false }));
      optionsRef.current.onError?.(error);
      return undefined;
    }
  }, []);

  const reset = useCallback(() => {
    runIdRef.current += 1;
    setState(idle);
  }, []);

  return { ...state, run, reset };
}

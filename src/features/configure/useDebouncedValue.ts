import { useEffect, useState } from "react";

/** Default delay used by the live key / URL checks on the Configure screen. */
export const DEFAULT_DEBOUNCE_MS = 300;

/**
 * Returns `value` after it has been stable for `delayMs`. The first render returns the initial
 * value immediately, so a pre-filled field (e.g. the preset base URL) is evaluated at once.
 */
export function useDebouncedValue<T>(value: T, delayMs: number = DEFAULT_DEBOUNCE_MS): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);
  return debounced;
}

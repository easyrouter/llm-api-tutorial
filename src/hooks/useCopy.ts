/**
 * `useCopy` — copy-to-clipboard with a transient "copied" flag for button feedback.
 *
 * ```tsx
 * const { copied, failed, copy } = useCopy();
 * <Button onClick={() => void copy(value)}>{copied ? t("actions.copied") : t("actions.copy")}</Button>
 * ```
 * The flag resets after `resetMs` (default 1.5 s) and any pending timer is cleared on unmount.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { copyText } from "@/lib/clipboard";

export const DEFAULT_COPIED_RESET_MS = 1500;

export interface UseCopyResult {
  /** True for `resetMs` after a successful copy. */
  copied: boolean;
  /** True for `resetMs` after a failed copy (clipboard unavailable). */
  failed: boolean;
  /** Copies `text`; resolves `true` on success. Never throws. */
  copy: (text: string) => Promise<boolean>;
}

type Outcome = "idle" | "copied" | "failed";

export function useCopy(resetMs: number = DEFAULT_COPIED_RESET_MS): UseCopyResult {
  const [outcome, setOutcome] = useState<Outcome>("idle");
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const flash = useCallback(
    (next: Outcome) => {
      if (!mountedRef.current) return;
      setOutcome(next);
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        if (mountedRef.current) setOutcome("idle");
      }, resetMs);
    },
    [resetMs],
  );

  const copy = useCallback(
    async (text: string) => {
      try {
        await copyText(text);
        flash("copied");
        return true;
      } catch {
        flash("failed");
        return false;
      }
    },
    [flash],
  );

  return { copied: outcome === "copied", failed: outcome === "failed", copy };
}

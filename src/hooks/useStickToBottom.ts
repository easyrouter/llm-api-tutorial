/**
 * `useStickToBottom` — keeps a scrollable container pinned to its bottom while new content
 * arrives, unless the user has scrolled up to read (then it leaves them alone until they scroll
 * back down). Used by `LogView`; reusable for any streaming list.
 *
 * ```tsx
 * const { ref, onScroll } = useStickToBottom<HTMLDivElement>(lines.length, autoScroll);
 * <div ref={ref} onScroll={onScroll} className="overflow-y-auto">…</div>
 * ```
 */
import { useCallback, useEffect, useRef, type RefObject, type UIEvent } from "react";

/** How close (px) to the bottom still counts as "at the bottom". */
export const STICK_THRESHOLD_PX = 24;

/** True when the element is scrolled to (or within `threshold` of) its bottom. */
export function isScrolledToBottom(el: HTMLElement, threshold = STICK_THRESHOLD_PX): boolean {
  return el.scrollHeight - el.scrollTop - el.clientHeight <= threshold;
}

export interface StickToBottom<T extends HTMLElement> {
  ref: RefObject<T | null>;
  /** Attach to the container's `onScroll` so user scrolling is tracked. */
  onScroll: (event: UIEvent<T>) => void;
}

/**
 * @param signal any value that changes when new content is appended (e.g. `lines.length`)
 * @param enabled set to `false` to disable auto-scrolling entirely
 */
export function useStickToBottom<T extends HTMLElement>(
  signal: unknown,
  enabled = true,
): StickToBottom<T> {
  const ref = useRef<T | null>(null);
  const stickRef = useRef(true);

  const onScroll = useCallback((event: UIEvent<T>) => {
    stickRef.current = isScrolledToBottom(event.currentTarget);
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!enabled || !el || !stickRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [signal, enabled]);

  return { ref, onScroll };
}

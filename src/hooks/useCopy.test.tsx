import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { mockWriteText } from "@/test/mocks/tauri";

import { useCopy } from "./useCopy";

describe("useCopy", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("copies through lib/clipboard and flashes copied for the reset window", async () => {
    const { result } = renderHook(() => useCopy(1500));
    expect(result.current.copied).toBe(false);

    let ok = false;
    await act(async () => {
      ok = await result.current.copy("hello");
    });
    expect(ok).toBe(true);
    expect(mockWriteText).toHaveBeenCalledWith("hello");
    expect(result.current.copied).toBe(true);

    act(() => {
      vi.advanceTimersByTime(1499);
    });
    expect(result.current.copied).toBe(true);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current.copied).toBe(false);
  });

  it("reports failure without throwing", async () => {
    mockWriteText.mockRejectedValueOnce(new Error("no clipboard"));
    const { result } = renderHook(() => useCopy());
    let ok = true;
    await act(async () => {
      ok = await result.current.copy("x");
    });
    expect(ok).toBe(false);
    expect(result.current.failed).toBe(true);
    expect(result.current.copied).toBe(false);
  });

  it("does not update after unmount", async () => {
    const { result, unmount } = renderHook(() => useCopy(100));
    await act(async () => {
      await result.current.copy("x");
    });
    unmount();
    expect(() => vi.advanceTimersByTime(200)).not.toThrow();
  });
});

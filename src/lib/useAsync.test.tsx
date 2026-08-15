import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { useAsync } from "./useAsync";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("useAsync", () => {
  it("starts idle", () => {
    const { result } = renderHook(() => useAsync(() => Promise.resolve(1)));
    expect(result.current).toMatchObject({ data: null, error: null, loading: false });
  });

  it("tracks loading, then data, and resolves run() with the value", async () => {
    const d = deferred<string>();
    const onSuccess = vi.fn();
    const { result } = renderHook(() => useAsync(() => d.promise, { onSuccess }));

    let outcome: Promise<string | undefined>;
    act(() => {
      outcome = result.current.run();
    });
    expect(result.current.loading).toBe(true);

    await act(async () => {
      d.resolve("ok");
      await expect(outcome).resolves.toBe("ok");
    });
    expect(result.current).toMatchObject({ data: "ok", error: null, loading: false });
    expect(onSuccess).toHaveBeenCalledWith("ok");
  });

  it("normalises rejections into WireError and never throws", async () => {
    const onError = vi.fn();
    const d = deferred<string>();
    const { result } = renderHook(() => useAsync(() => d.promise, { onError }));
    let outcome: string | undefined = "sentinel";
    await act(async () => {
      const pending = result.current.run();
      d.reject({ code: "network", message: "connect", params: {} });
      outcome = await pending;
    });
    expect(outcome).toBeUndefined();
    expect(result.current.error).toEqual({ code: "network", message: "connect", params: {} });
    expect(result.current.loading).toBe(false);
    expect(onError).toHaveBeenCalledWith({ code: "network", message: "connect", params: {} });
  });

  it("wraps plain Error rejections with code other", async () => {
    const { result } = renderHook(() => useAsync(() => Promise.reject(new Error("boom"))));
    await act(async () => {
      await result.current.run();
    });
    expect(result.current.error).toEqual({ code: "other", message: "boom", params: {} });
  });

  it("passes arguments through and keeps run() stable across renders", async () => {
    const fn = vi.fn((a: number, b: string) => Promise.resolve(`${a}-${b}`));
    const { result, rerender } = renderHook(() => useAsync(fn));
    const firstRun = result.current.run;
    rerender();
    expect(result.current.run).toBe(firstRun);
    await act(async () => {
      await result.current.run(7, "x");
    });
    expect(fn).toHaveBeenCalledWith(7, "x");
    expect(result.current.data).toBe("7-x");
  });

  it("uses the latest fn without recreating run()", async () => {
    let value = "first";
    const { result, rerender } = renderHook(() => useAsync(() => Promise.resolve(value)));
    value = "second";
    rerender();
    await act(async () => {
      await result.current.run();
    });
    expect(result.current.data).toBe("second");
  });

  it("ignores stale results when a newer run started", async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    const calls = [first, second];
    const { result } = renderHook(() => useAsync(() => calls.shift()!.promise));

    act(() => {
      void result.current.run();
    });
    act(() => {
      void result.current.run();
    });
    await act(async () => {
      second.resolve("newer");
      await Promise.resolve();
    });
    await waitFor(() => expect(result.current.data).toBe("newer"));
    await act(async () => {
      first.resolve("older");
      await Promise.resolve();
    });
    expect(result.current.data).toBe("newer");
    expect(result.current.loading).toBe(false);
  });

  it("ignores results after unmount (no state update, no callbacks)", async () => {
    const d = deferred<number>();
    const onSuccess = vi.fn();
    const { result, unmount } = renderHook(() => useAsync(() => d.promise, { onSuccess }));
    let outcome: Promise<number | undefined>;
    act(() => {
      outcome = result.current.run();
    });
    unmount();
    d.resolve(1);
    await expect(outcome!).resolves.toBeUndefined();
    expect(onSuccess).not.toHaveBeenCalled();
  });

  it("reset() clears state and cancels the in-flight run", async () => {
    const d = deferred<number>();
    const { result } = renderHook(() => useAsync(() => d.promise));
    act(() => {
      void result.current.run();
    });
    act(() => {
      result.current.reset();
    });
    expect(result.current).toMatchObject({ data: null, error: null, loading: false });
    await act(async () => {
      d.resolve(5);
      await Promise.resolve();
    });
    expect(result.current.data).toBeNull();
  });
});

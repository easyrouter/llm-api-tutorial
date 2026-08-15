import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { isScrolledToBottom, useStickToBottom } from "./useStickToBottom";

/** jsdom has no layout: fake the scroll geometry of an element. */
function fakeGeometry(el: HTMLElement, scrollHeight: number, clientHeight: number) {
  Object.defineProperty(el, "scrollHeight", { configurable: true, get: () => scrollHeight });
  Object.defineProperty(el, "clientHeight", { configurable: true, get: () => clientHeight });
}

function Box({ count, enabled = true }: { count: number; enabled?: boolean }) {
  const { ref, onScroll } = useStickToBottom<HTMLDivElement>(count, enabled);
  return (
    <div ref={ref} onScroll={onScroll} data-testid="box">
      {count}
    </div>
  );
}

describe("isScrolledToBottom", () => {
  it.each([
    [1000, 900, 100, true],
    [1000, 880, 100, true],
    [1000, 800, 100, false],
    [50, 0, 100, true],
  ])("scrollHeight %s scrollTop %s clientHeight %s → %s", (sh, st, ch, expected) => {
    const el = document.createElement("div");
    fakeGeometry(el, sh, ch);
    el.scrollTop = st;
    expect(isScrolledToBottom(el)).toBe(expected);
  });
});

describe("useStickToBottom", () => {
  it("scrolls to the bottom when the signal changes", () => {
    const { getByTestId, rerender } = render(<Box count={1} />);
    const box = getByTestId("box");
    fakeGeometry(box, 500, 100);
    rerender(<Box count={2} />);
    expect(box.scrollTop).toBe(500);
  });

  it("stops following once the user scrolls up, and resumes at the bottom", () => {
    const { getByTestId, rerender } = render(<Box count={1} />);
    const box = getByTestId("box");
    fakeGeometry(box, 500, 100);

    box.scrollTop = 100;
    fireEvent.scroll(box);
    rerender(<Box count={2} />);
    expect(box.scrollTop).toBe(100);

    box.scrollTop = 400;
    fireEvent.scroll(box);
    rerender(<Box count={3} />);
    expect(box.scrollTop).toBe(500);
  });

  it("does nothing when disabled", () => {
    const { getByTestId, rerender } = render(<Box count={1} enabled={false} />);
    const box = getByTestId("box");
    fakeGeometry(box, 500, 100);
    rerender(<Box count={2} enabled={false} />);
    expect(box.scrollTop).toBe(0);
  });
});

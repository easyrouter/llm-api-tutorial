import { render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";

import i18n from "@/i18n";

import { LogView, MAX_RENDERED_LOG_LINES, type LogLine } from "./LogView";

describe("LogView", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("shows an empty state", () => {
    render(<LogView lines={[]} />);
    expect(screen.getByRole("log")).toHaveTextContent("No output yet");
  });

  it("tints stderr and system lines", () => {
    const lines: LogLine[] = [
      { stream: "stdout", line: "added 1 package" },
      { stream: "stderr", line: "npm WARN deprecated" },
      { stream: "system", line: "> npm install -g @openai/codex" },
    ];
    const { container } = render(<LogView lines={lines} />);
    expect(container.querySelector('[data-stream="stdout"]')).toHaveClass("text-neutral-100");
    expect(container.querySelector('[data-stream="stderr"]')).toHaveClass("text-warning-500");
    const system = container.querySelector('[data-stream="system"]');
    expect(system).toHaveClass("italic");
    expect(system).toHaveClass("text-brand-100");
  });

  it("caps rendering to the last 2000 lines and says how many are hidden", () => {
    const lines: LogLine[] = Array.from({ length: MAX_RENDERED_LOG_LINES + 25 }, (_, i) => ({
      stream: "stdout",
      line: `line ${i}`,
    }));
    const { container } = render(<LogView lines={lines} />);
    expect(container.querySelectorAll("[data-stream]")).toHaveLength(MAX_RENDERED_LOG_LINES);
    expect(screen.getByTestId("log-truncated")).toHaveTextContent("25 earlier lines hidden");
    expect(screen.queryByText("line 24")).toBeNull();
    expect(screen.getByText("line 25")).toBeInTheDocument();
    expect(screen.getByText(`line ${MAX_RENDERED_LOG_LINES + 24}`)).toBeInTheDocument();
  });

  it("applies maxHeight and keeps the pane scrollable + selectable", () => {
    render(<LogView lines={[]} maxHeight={120} />);
    const log = screen.getByRole("log");
    expect(log).toHaveStyle({ maxHeight: "120px" });
    expect(log).toHaveClass("overflow-y-auto");
    expect(log).toHaveAttribute("data-selectable");
  });
});

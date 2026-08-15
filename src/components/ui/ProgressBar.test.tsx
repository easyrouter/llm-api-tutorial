import { render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";

import i18n from "@/i18n";

import { ProgressBar } from "./ProgressBar";

describe("ProgressBar", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders a determinate bar with percentage", () => {
    render(<ProgressBar value={0.42} label="Downloading" />);
    const bar = screen.getByRole("progressbar", { name: "Downloading" });
    expect(bar).toHaveAttribute("aria-valuenow", "42");
    expect(bar).toHaveAttribute("aria-busy", "false");
    expect(screen.getByTestId("progress-fill")).toHaveStyle({ width: "42%" });
    expect(screen.getByText("42%")).toBeInTheDocument();
  });

  it("clamps out-of-range values", () => {
    render(<ProgressBar value={1.7} />);
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
    expect(screen.getByTestId("progress-fill")).toHaveStyle({ width: "100%" });
  });

  it("renders indeterminate when value is undefined", () => {
    render(<ProgressBar />);
    const bar = screen.getByRole("progressbar", { name: "Progress" });
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).toHaveAttribute("aria-busy", "true");
    expect(bar).toHaveAttribute("aria-valuetext", "Working, please wait…");
    expect(screen.getByTestId("progress-fill")).toHaveClass("ui-progress-indeterminate");
  });

  it("prefers detail text over the percentage", () => {
    render(<ProgressBar value={0.5} detail="5 MB / 10 MB" />);
    expect(screen.getByText("5 MB / 10 MB")).toBeInTheDocument();
    expect(screen.queryByText("50%")).toBeNull();
  });
});

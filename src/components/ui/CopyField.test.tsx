import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import i18n from "@/i18n";
import { mockWriteText } from "@/test/mocks/tauri";

import { CopyField } from "./CopyField";

describe("CopyField", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows the value in a monospace box with a label", () => {
    render(<CopyField label="Base URL" value="https://gateway.example.com/v1" />);
    const box = screen.getByTestId("copy-field-value");
    expect(box).toHaveTextContent("https://gateway.example.com/v1");
    expect(box).toHaveClass("font-mono");
    expect(screen.getByText("Base URL")).toBeInTheDocument();
  });

  it("copies the value and shows Copied for 1.5 s", async () => {
    render(<CopyField value="npm install -g @openai/codex" />);
    const button = screen.getByRole("button", { name: "Copy" });
    await act(async () => {
      fireEvent.click(button);
      await Promise.resolve();
    });
    expect(mockWriteText).toHaveBeenCalledWith("npm install -g @openai/codex");
    expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(screen.getByRole("button", { name: "Copy" })).toBeInTheDocument();
  });

  it("masks secrets on screen but copies the real value", async () => {
    render(<CopyField value="sk-abcdefghijkl" secret />);
    const box = screen.getByTestId("copy-field-value");
    expect(box).toHaveTextContent("sk-****ijkl");
    expect(box).not.toHaveTextContent("sk-abcdefghijkl");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Copy" }));
      await Promise.resolve();
    });
    expect(mockWriteText).toHaveBeenCalledWith("sk-abcdefghijkl");
  });

  it("reports a failed copy", async () => {
    mockWriteText.mockRejectedValueOnce(new Error("denied"));
    render(<CopyField value="x" />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Copy" }));
      await Promise.resolve();
    });
    expect(screen.getByRole("status")).toHaveTextContent(/Copy failed/);
  });

  it("can render proportional text", () => {
    render(<CopyField value="Service gateway" mono={false} />);
    expect(screen.getByTestId("copy-field-value")).not.toHaveClass("font-mono");
  });
});

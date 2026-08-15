import { fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";

import i18n from "@/i18n";

import { ConfirmDialog } from "./ConfirmDialog";

describe("ConfirmDialog", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders nothing when closed", () => {
    render(
      <ConfirmDialog
        open={false}
        title="Run?"
        onConfirm={() => undefined}
        onCancel={() => undefined}
      />,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("is an accessible modal portalled to body and focuses the first button", () => {
    render(
      <ConfirmDialog
        open
        title="Run install?"
        description="npm install -g @openai/codex"
        onConfirm={() => undefined}
        onCancel={() => undefined}
      >
        <code>extra</code>
      </ConfirmDialog>,
    );
    const dialog = screen.getByRole("dialog", { name: "Run install?" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleDescription("npm install -g @openai/codex");
    expect(dialog.parentElement?.parentElement).toBe(document.body);
    expect(screen.getByText("extra")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
  });

  it("calls onConfirm / onCancel from the buttons with custom labels", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        open
        title="T"
        confirmLabel="Yes, run it"
        cancelLabel="No"
        danger
        onConfirm={onConfirm}
        onCancel={onCancel}
      />,
    );
    const confirm = screen.getByRole("button", { name: "Yes, run it" });
    expect(confirm).toHaveClass("bg-danger-500");
    fireEvent.click(confirm);
    fireEvent.click(screen.getByRole("button", { name: "No" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("cancels on Escape and on backdrop click, but not on clicks inside", () => {
    const onCancel = vi.fn();
    render(<ConfirmDialog open title="T" onConfirm={() => undefined} onCancel={onCancel} />);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.mouseDown(screen.getByRole("dialog"));
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.mouseDown(screen.getByTestId("dialog-backdrop"));
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it("keeps Tab focus inside the dialog", () => {
    render(<ConfirmDialog open title="T" onConfirm={() => undefined} onCancel={() => undefined} />);
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const confirm = screen.getByRole("button", { name: "Confirm" });
    confirm.focus();
    fireEvent.keyDown(confirm, { key: "Tab" });
    expect(cancel).toHaveFocus();
    fireEvent.keyDown(cancel, { key: "Tab", shiftKey: true });
    expect(confirm).toHaveFocus();
  });

  it("restores focus to the opener when closed", () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const { rerender } = render(
      <ConfirmDialog open title="T" onConfirm={() => undefined} onCancel={() => undefined} />,
    );
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    rerender(
      <ConfirmDialog
        open={false}
        title="T"
        onConfirm={() => undefined}
        onCancel={() => undefined}
      />,
    );
    expect(opener).toHaveFocus();
    opener.remove();
  });
});

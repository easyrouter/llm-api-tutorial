import { fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";

import i18n from "@/i18n";
import { wireError } from "@/test/mocks/tauri";

import { ErrorBanner } from "./ErrorBanner";

describe("ErrorBanner", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders nothing without an error", () => {
    const { container } = render(<ErrorBanner error={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("describes a wire error with its code and offers Retry", () => {
    const onRetry = vi.fn();
    render(
      <ErrorBanner
        error={wireError("command_not_found", "raw", { program: "codex" })}
        onRetry={onRetry}
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Something went wrong");
    expect(alert).toHaveTextContent("Command codex was not found.");
    expect(alert).toHaveTextContent("Error code: command_not_found");
    expect(alert).toHaveAttribute("data-error-code", "command_not_found");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("shows the raw message for JS errors and hides the meaningless code", () => {
    render(<ErrorBanner error={new Error("kaboom")} />);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Operation failed: kaboom");
    expect(alert).not.toHaveTextContent("Error code");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("accepts a custom title and extra actions", () => {
    render(
      <ErrorBanner
        error="x"
        title="Verify failed"
        actions={<button type="button">Open help</button>}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Verify failed");
    expect(screen.getByRole("button", { name: "Open help" })).toBeInTheDocument();
  });
});

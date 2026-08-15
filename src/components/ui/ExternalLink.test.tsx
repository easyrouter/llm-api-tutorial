import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import { mockInvoke, rejectWith, setInvokeHandlers, wireError } from "@/test/mocks/tauri";

import { ExternalLink } from "./ExternalLink";

describe("ExternalLink", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("opens the URL through the open_external command (never an <a href>)", () => {
    render(<ExternalLink href="https://nodejs.org/">Node.js</ExternalLink>);
    const button = screen.getByRole("button", { name: /Node\.js/ });
    expect(button.tagName).toBe("BUTTON");
    expect(document.querySelector("a")).toBeNull();
    fireEvent.click(button);
    expect(mockInvoke).toHaveBeenCalledWith("open_external", { url: "https://nodejs.org/" });
  });

  it("renders as a secondary button when asked", () => {
    render(
      <ExternalLink href="https://example.com" variant="button">
        Docs
      </ExternalLink>,
    );
    expect(screen.getByRole("button", { name: "Docs" })).toHaveClass("border");
  });

  it("marks the link when opening fails", async () => {
    setInvokeHandlers({ open_external: rejectWith(wireError("invalid_input", "bad url")) });
    render(<ExternalLink href="ftp://nope">nope</ExternalLink>);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /nope/ }));
      await Promise.resolve();
    });
    expect(screen.getByRole("button", { name: /nope/ })).toHaveAttribute(
      "title",
      "Could not open link: ftp://nope",
    );
  });
});

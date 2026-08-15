import { render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";

import i18n from "@/i18n";

import { StatusBadge, type BadgeStatus } from "./StatusBadge";

describe("StatusBadge", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it.each<[BadgeStatus, string, string]>([
    ["pass", "Pass", "text-success-500"],
    ["warn", "Warning", "text-warning-500"],
    ["fail", "Blocked", "text-danger-500"],
    ["skipped", "Skipped", "text-neutral-600"],
    ["pending", "Pending", "text-neutral-500"],
    ["running", "Checking…", "text-brand-700"],
  ])("renders %s with its label and colour", (status, label, cls) => {
    const { container } = render(<StatusBadge status={status} />);
    const badge = container.querySelector(`[data-status="${status}"]`);
    expect(badge).toHaveTextContent(label);
    expect(badge).toHaveClass(cls);
  });

  it("shows a spinner while running", () => {
    const { container } = render(<StatusBadge status="running" />);
    expect(container.querySelector(".animate-spin")).not.toBeNull();
  });

  it("supports a custom label and compact mode", () => {
    render(<StatusBadge status="pass" label="Node.js 22" compact />);
    const text = screen.getByText("Node.js 22");
    expect(text).toHaveClass("sr-only");
  });

  it("uses zh-CN labels when the language changes", async () => {
    await i18n.changeLanguage("zh-CN");
    render(<StatusBadge status="fail" />);
    expect(screen.getByText("阻断")).toBeInTheDocument();
    await i18n.changeLanguage("en");
  });
});

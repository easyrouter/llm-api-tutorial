import { render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";

import i18n from "@/i18n";

import { Spinner } from "./Spinner";

describe("Spinner", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("is a labelled status by default", () => {
    render(<Spinner />);
    expect(screen.getByRole("status", { name: "Loading…" })).toBeInTheDocument();
  });

  it("accepts a custom label and size", () => {
    const { container } = render(<Spinner label="Checking Node.js" size="lg" />);
    expect(screen.getByRole("status", { name: "Checking Node.js" })).toBeInTheDocument();
    expect(container.querySelector("svg")).toHaveClass("size-10");
  });

  it("can be purely decorative", () => {
    const { container } = render(<Spinner decorative />);
    expect(screen.queryByRole("status")).toBeNull();
    expect(container.firstElementChild).toHaveAttribute("aria-hidden", "true");
  });
});

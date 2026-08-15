import { fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";

import i18n from "@/i18n";

import { StepFooter } from "./StepFooter";

describe("StepFooter", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders Back / extra / Next and wires the handlers", () => {
    const onBack = vi.fn();
    const onNext = vi.fn();
    render(<StepFooter onBack={onBack} onNext={onNext} extra={<span>middle</span>} />);
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(onBack).toHaveBeenCalledTimes(1);
    expect(onNext).toHaveBeenCalledTimes(1);
    expect(screen.getByText("middle")).toBeInTheDocument();
  });

  it("hides buttons without handlers and supports custom labels / disabled state", () => {
    render(<StepFooter onNext={() => undefined} nextLabel="Start checks" nextDisabled />);
    expect(screen.queryByRole("button", { name: "Back" })).toBeNull();
    expect(screen.getByRole("button", { name: "Start checks" })).toBeDisabled();
  });

  it("disables Next while loading", () => {
    render(<StepFooter onNext={() => undefined} nextLoading />);
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
  });
});

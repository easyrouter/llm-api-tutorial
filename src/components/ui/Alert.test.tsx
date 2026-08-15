import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Alert, type AlertVariant } from "./Alert";

describe("Alert", () => {
  it.each<[AlertVariant, string]>([
    ["info", "status"],
    ["success", "status"],
    ["warning", "alert"],
    ["danger", "alert"],
  ])("%s uses role=%s by default", (variant, role) => {
    render(
      <Alert variant={variant} title="Title">
        Body
      </Alert>,
    );
    const el = screen.getByRole(role);
    expect(el).toHaveAttribute("data-variant", variant);
    expect(el).toHaveTextContent("Title");
    expect(el).toHaveTextContent("Body");
  });

  it("renders actions and honours an explicit role", () => {
    render(
      <Alert variant="danger" role="note" actions={<button type="button">Fix</button>}>
        Body
      </Alert>,
    );
    expect(screen.getByRole("note")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Fix" })).toBeInTheDocument();
  });

  it("can hide the icon", () => {
    const { container } = render(<Alert hideIcon>Body</Alert>);
    expect(container.querySelector("svg")).toBeNull();
  });
});

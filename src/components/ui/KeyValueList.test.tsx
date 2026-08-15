import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { KeyValueList } from "./KeyValueList";

describe("KeyValueList", () => {
  it("renders nothing for an empty list", () => {
    const { container } = render(<KeyValueList items={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders label/value pairs, mono values in monospace", () => {
    render(
      <KeyValueList
        items={[
          { label: "OS", value: "Windows 11" },
          { label: "Node path", value: "C:\\nodejs\\node.exe", mono: true },
        ]}
      />,
    );
    expect(screen.getByText("OS").tagName).toBe("DT");
    expect(screen.getByText("Windows 11").tagName).toBe("DD");
    expect(screen.getByText("Windows 11")).not.toHaveClass("font-mono");
    expect(screen.getByText("C:\\nodejs\\node.exe")).toHaveClass("font-mono");
  });

  it("supports two columns", () => {
    const { container } = render(<KeyValueList columns={2} items={[{ label: "a", value: "1" }]} />);
    expect(container.querySelector("dl")).toHaveClass("sm:grid-cols-2");
  });
});

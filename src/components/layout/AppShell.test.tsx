import { fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import { useAppStore } from "@/stores/app";
import { useWizardStore } from "@/stores/wizard";
import { mockInvoke } from "@/test/mocks/tauri";

import { AppShell } from "./AppShell";

describe("AppShell", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  beforeEach(() => {
    useWizardStore.getState().reset();
    useAppStore.setState({
      info: {
        name: "SeedRouter Onboarding",
        version: "0.1.0",
        platform: "windows",
        arch: "x86_64",
        localeHint: "en",
        configSource: "bundled",
        logDir: "C:/logs",
      },
      config: null,
    });
  });

  it("opens the brand site from the header link next to the version", () => {
    render(<AppShell>body</AppShell>);
    expect(screen.getByText("Version 0.1.0")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /seedrouter\.net/ }));
    expect(mockInvoke).toHaveBeenCalledWith("open_external", { url: "https://seedrouter.net" });
  });

  it("credits seedrouter.net in the footer instead of the config source", () => {
    render(<AppShell>body</AppShell>);
    const year = new Date().getFullYear();
    expect(screen.getByText(`© ${year} seedrouter.net. All rights reserved.`)).toBeInTheDocument();
    expect(screen.queryByText(/Config source/)).toBeNull();
  });
});

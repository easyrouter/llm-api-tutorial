import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import i18n from "@/i18n";
import type { Diagnosis, DiagnosticReport, EnvSnapshot } from "@/lib/types";
import { useWizardStore } from "@/stores/wizard";
import {
  mockInvoke,
  mockWriteText,
  rejectWith,
  setInvokeHandlers,
  wireError,
} from "@/test/mocks/tauri";

import { DiagnosePanel } from "./DiagnosePanel";

const { mockSave } = vi.hoisted(() => ({
  mockSave: vi.fn((): Promise<string | null> => Promise.resolve(null)),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: mockSave }));

const REPORT_MD = "# SeedRouter Onboarding diagnostic report\n\n- app: 0.1.0\n";

const auth: Diagnosis = {
  ruleId: "A",
  severity: "blocking",
  code: "auth.key_checklist",
  params: { status: "401", tool: "codex" },
  actions: [{ kind: "go_to_step", step: "configure" }, { kind: "rerun" }],
  checklist: ["whitespace", "provider_selected", "key_valid"],
};

const session: Diagnosis = {
  ruleId: "C",
  severity: "warning",
  code: "session.restart_terminal",
  params: {},
  actions: [],
  // `retry` is not defined under session.restart_terminal → shared steps.* fallback
  checklist: ["close_all_terminals", "retry"],
};

const envConflict: Diagnosis = {
  ruleId: "D",
  severity: "warning",
  code: "env.conflict",
  params: { name: "OPENAI_API_KEY" },
  actions: [
    { kind: "instructions", code: "diagnose:env.conflict.instructions.windows", params: {} },
  ],
  checklist: ["inspect", "remove_manually", "restart_terminal"],
};

const snapshot = { generatedAt: "2026-01-01T00:00:00Z" } as unknown as EnvSnapshot;

describe("DiagnosePanel", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  beforeEach(() => {
    useWizardStore.getState().reset();
    mockSave.mockReset();
    mockSave.mockResolvedValue(null);
    setInvokeHandlers({
      build_diagnostic_report: (): DiagnosticReport => ({
        generatedAt: "2026-01-01T00:00:00Z",
        appVersion: "0.1.0",
        markdown: REPORT_MD,
      }),
    });
  });

  it("renders diagnoses sorted by severity with title, explanation, checklist and details", () => {
    render(<DiagnosePanel diagnoses={[session, auth]} snapshot={null} />);
    const cards = screen.getAllByRole("article");
    expect(cards.map((c) => c.getAttribute("data-rule-id"))).toEqual(["A", "C"]);

    const authCard = cards[0]!;
    expect(within(authCard).getByText("Blocking")).toBeInTheDocument();
    expect(within(authCard).getByText("Fault A")).toBeInTheDocument();
    expect(within(authCard).getByRole("heading", { level: 3 })).toHaveTextContent(
      /Authentication failed/,
    );
    expect(within(authCard).getByText(/refused the key/)).toBeInTheDocument();
    const steps = within(authCard).getAllByRole("listitem");
    expect(steps).toHaveLength(3);
    expect(steps[0]).toHaveTextContent(/Stray spaces or line breaks/);
    expect(steps[1]).toHaveTextContent(/Wrong active provider/);
    // params rendered as labelled details
    expect(within(authCard).getByText("HTTP status")).toBeInTheDocument();
    expect(within(authCard).getByText("401")).toBeInTheDocument();
    // fix actions rendered (go_to_step + rerun hidden without handler)
    expect(within(authCard).getByRole("button", { name: /Go to “Configure”/ })).toBeInTheDocument();
    expect(within(authCard).queryByRole("button", { name: "Re-check" })).toBeNull();

    const sessionCard = cards[1]!;
    expect(within(sessionCard).getByText("Warning")).toBeInTheDocument();
    const sessionSteps = within(sessionCard).getAllByRole("listitem");
    expect(sessionSteps[0]).toHaveTextContent(/Close every terminal window/);
    // shared fallback for a step that this rule does not define itself
    expect(sessionSteps[1]).toHaveTextContent("Wait a moment and try again.");
  });

  it("wires rerun fixes and instructions actions", () => {
    const onRerun = vi.fn();
    render(<DiagnosePanel diagnoses={[auth, envConflict]} snapshot={null} onRerun={onRerun} />);
    fireEvent.click(screen.getByRole("button", { name: "Re-check" }));
    expect(onRerun).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Show instructions" }));
    expect(screen.getByText(/Advanced system settings/)).toBeInTheDocument();
  });

  it("rule E (path.not_refreshed): the one-click PATH repair opens its plan dialog", async () => {
    const pathRule: Diagnosis = {
      ruleId: "E",
      severity: "blocking",
      code: "path.not_refreshed",
      params: { tool: "codex", binary: "codex", dir: "C:\\Users\\alice\\AppData\\Roaming\\npm" },
      actions: [
        { kind: "repair_path", dir: "C:\\Users\\alice\\AppData\\Roaming\\npm" },
        { kind: "go_to_step", step: "install" },
        { kind: "rerun" },
      ],
      checklist: ["restart_terminal"],
    };
    setInvokeHandlers({
      plan_path_repair: (args) => ({
        dir: args?.dir,
        platform: "windows",
        location: "HKCU\\Environment\\Path",
        displayCommand: "setx Path …",
        alreadyPresent: false,
        createsBackup: false,
      }),
    });
    render(<DiagnosePanel diagnoses={[pathRule]} snapshot={null} onRerun={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Fix PATH now" }));
    expect(mockInvoke).toHaveBeenCalledWith("plan_path_repair", {
      dir: "C:\\Users\\alice\\AppData\\Roaming\\npm",
    });
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("HKCU\\Environment\\Path");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows an empty state and the redaction note", () => {
    render(<DiagnosePanel diagnoses={[]} snapshot={null} />);
    expect(screen.getByText("No diagnosis available yet.")).toBeInTheDocument();
    expect(screen.getByText(/automatically redacted/)).toBeInTheDocument();
  });

  it("copies the redacted report built by Rust", async () => {
    render(<DiagnosePanel diagnoses={[auth]} snapshot={snapshot} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Copy diagnostic report" }));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(mockInvoke).toHaveBeenCalledWith("build_diagnostic_report", {
      snapshot,
      diagnoses: [auth],
      verify: [],
    });
    expect(mockWriteText).toHaveBeenCalledWith(REPORT_MD);
    expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
  });

  it("exports the report through the save dialog", async () => {
    mockSave.mockResolvedValue("C:\\Users\\me\\Desktop\\seedrouter-onboarding-report.md");
    render(<DiagnosePanel diagnoses={[auth]} snapshot={snapshot} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Export report…" }));
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(mockSave).toHaveBeenCalledWith({
      defaultPath: "seedrouter-onboarding-report.md",
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    expect(mockInvoke).toHaveBeenCalledWith("save_diagnostic_report", {
      path: "C:\\Users\\me\\Desktop\\seedrouter-onboarding-report.md",
      markdown: REPORT_MD,
    });
    expect(await screen.findByText(/Report saved to:/)).toBeInTheDocument();
  });

  it("reports a cancelled export without writing anything", async () => {
    render(<DiagnosePanel diagnoses={[auth]} snapshot={null} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Export report…" }));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(await screen.findByText("Export cancelled.")).toBeInTheDocument();
    expect(mockInvoke).not.toHaveBeenCalledWith("save_diagnostic_report", expect.anything());
  });

  it("surfaces a Rust error while building the report", async () => {
    setInvokeHandlers({ build_diagnostic_report: rejectWith(wireError("io")) });
    render(<DiagnosePanel diagnoses={[auth]} snapshot={null} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Copy diagnostic report" }));
      await Promise.resolve();
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(/File read\/write failed/);
    expect(mockWriteText).not.toHaveBeenCalled();
  });
});

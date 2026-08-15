import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import { EVENTS } from "@/lib/events";
import type { AppConfig, InstallJob, InstallTarget, TelemetryEvent } from "@/lib/types";
import { useAppStore } from "@/stores/app";
import { useInstallStore } from "@/stores/install";
import { useWizardStore } from "@/stores/wizard";
import {
  ccSwitchRelease,
  checkResult,
  downloadResult,
  envSnapshot,
  installPlan,
} from "@/test/fixtures/env";
import {
  emitMockEvent,
  listenerCount,
  mockInvoke,
  rejectWith,
  setInvokeHandlers,
  wireError,
} from "@/test/mocks/tauri";

import { InstallScreen } from "./InstallScreen";

function card(target: InstallTarget) {
  return screen.getByTestId(`install-item-${target}`);
}

function calls(cmd: string) {
  return mockInvoke.mock.calls.filter((c) => c[0] === cmd);
}

describe("InstallScreen", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  beforeEach(() => {
    useWizardStore.getState().reset();
    useInstallStore.getState().reset();
    useAppStore.setState({
      info: {
        name: "t",
        version: "0",
        platform: "windows",
        arch: "x86_64",
        localeHint: "en",
        configSource: "bundled",
        logDir: "",
      },
      config: null,
    });
    useWizardStore.getState().goTo("install");
  });

  it("shows a success note and enables Next when nothing is missing", () => {
    useWizardStore.getState().setSnapshot(envSnapshot());
    render(<InstallScreen />);
    expect(screen.getByText("Everything required is already installed")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next" })).toBeEnabled();
    expect(calls("plan_install")).toHaveLength(0);
  });

  it("derives targets from the snapshot (selected tools only) plus explicit requests", async () => {
    useWizardStore.getState().setSelectedTools(["codex"]);
    useWizardStore.getState().setSnapshot(
      envSnapshot({
        node: checkResult("node", "fail", { code: "node.missing" }),
        codex: checkResult("codex", "fail", { code: "tool.missing" }),
        claude_code: checkResult("claude_code", "fail", { code: "tool.missing" }),
      }),
    );
    useInstallStore.getState().request("cc-switch");
    setInvokeHandlers({
      plan_install: (args) => installPlan(args?.target as InstallTarget),
      fetch_cc_switch_release: () => ccSwitchRelease(),
    });
    render(<InstallScreen />);

    expect(card("node")).toBeInTheDocument();
    expect(card("codex")).toBeInTheDocument();
    expect(card("cc-switch")).toBeInTheDocument();
    expect(screen.queryByTestId("install-item-claude-code")).toBeNull();
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();

    // every item is prepared once on mount
    await waitFor(() => expect(card("codex")).toHaveAttribute("data-phase", "confirm"));
    expect(calls("plan_install").map((c) => c[1])).toEqual([
      { target: "node", excludeRegistry: null },
      { target: "codex", excludeRegistry: null },
    ]);
    expect(calls("fetch_cc_switch_release")).toHaveLength(1);
    expect(listenerCount(EVENTS.installOutput)).toBe(1);
    expect(listenerCount(EVENTS.installDone)).toBe(1);
    expect(listenerCount(EVENTS.downloadProgress)).toBe(1);
  });

  it("npm target: confirm shows the exact command, Run streams output, done auto re-checks", async () => {
    useWizardStore
      .getState()
      .setSnapshot(envSnapshot({ codex: checkResult("codex", "fail", { code: "tool.missing" }) }));
    const plan = installPlan("codex", { requiresAdmin: true });
    const job: InstallJob = { jobId: "job-42", target: "codex" };
    setInvokeHandlers({
      plan_install: () => plan,
      start_install: () => job,
      run_env_check: () =>
        checkResult("codex", "pass", { code: "tool.ok", params: { version: "1.2.3" } }),
    });
    render(<InstallScreen />);
    const codex = card("codex");

    await waitFor(() => expect(codex).toHaveAttribute("data-phase", "confirm"));
    // first copy field = the command that will run; the second is the manual npm-prefix command
    const copyFields = within(codex).getAllByTestId("copy-field-value");
    expect(copyFields[0]).toHaveTextContent(plan.displayCommand);
    expect(copyFields[1]).toHaveTextContent("npm config set prefix");
    expect(within(codex).getByTestId("plan-mirror")).toHaveTextContent("npmmirror (China mirror)");
    expect(
      within(codex).getByText("The npm global folder needs administrator rights"),
    ).toBeInTheDocument();
    expect(calls("start_install")).toHaveLength(0);

    fireEvent.click(within(codex).getByRole("button", { name: "Confirm and run" }));
    expect(calls("start_install")[0]?.[1]).toEqual({ plan });
    await waitFor(() => expect(codex).toHaveAttribute("data-phase", "running"));
    // navigation is locked while the job runs
    expect(screen.getByRole("button", { name: "Back" })).toBeDisabled();
    expect(screen.getByTestId("navigation-locked")).toBeInTheDocument();
    expect(useWizardStore.getState().navigationLocked).toBe(true);

    act(() => {
      emitMockEvent(EVENTS.installOutput, {
        jobId: "job-42",
        stream: "stdout",
        line: "added 1 package",
      });
      emitMockEvent(EVENTS.installOutput, { jobId: "other", stream: "stdout", line: "not mine" });
    });
    expect(within(codex).getByRole("log")).toHaveTextContent("added 1 package");
    expect(within(codex).getByRole("log")).not.toHaveTextContent("not mine");
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();

    act(() => {
      emitMockEvent(EVENTS.installDone, {
        jobId: "job-42",
        success: true,
        exitCode: 0,
        durationMs: 3000,
        cancelled: false,
      });
    });
    expect(codex).toHaveAttribute("data-phase", "done");
    expect(screen.getByRole("button", { name: "Back" })).toBeEnabled();
    expect(useWizardStore.getState().navigationLocked).toBe(false);
    await waitFor(() => expect(calls("run_env_check")).toHaveLength(1));
    expect(calls("run_env_check")[0]?.[1]).toEqual({ id: "codex" });
    await waitFor(() =>
      expect(within(codex).getByTestId("recheck-feedback")).toHaveTextContent(
        "Codex CLI 1.2.3 is installed",
      ),
    );
    // the snapshot row was patched
    expect(useWizardStore.getState().snapshot?.checks.find((c) => c.id === "codex")?.status).toBe(
      "pass",
    );
    expect(screen.getByTestId("install-progress")).toHaveTextContent(
      "Every item in this step is handled",
    );
    expect(screen.getByRole("button", { name: "Next" })).toBeEnabled();
  });

  it("npm target: a failed job shows the exit code, the output and a retry that switches mirror", async () => {
    useWizardStore
      .getState()
      .setSnapshot(envSnapshot({ codex: checkResult("codex", "fail", { code: "tool.missing" }) }));
    useAppStore.setState({
      config: {
        mirrors: {
          npmRegistries: [
            { id: "official", url: "https://registry.npmjs.org/", downloadPage: "" },
            { id: "npmmirror", url: "https://registry.npmmirror.com/", downloadPage: "" },
          ],
          nodeDist: [],
          probeTimeoutMs: 1000,
        },
      } as unknown as AppConfig,
    });
    setInvokeHandlers({
      plan_install: () => installPlan("codex"),
      start_install: () => ({ jobId: "job-1", target: "codex" }),
    });
    render(<InstallScreen />);
    const codex = card("codex");
    await waitFor(() => expect(codex).toHaveAttribute("data-phase", "confirm"));
    fireEvent.click(within(codex).getByRole("button", { name: "Confirm and run" }));
    await waitFor(() => expect(codex).toHaveAttribute("data-phase", "running"));

    fireEvent.click(within(codex).getByRole("button", { name: "Cancel install" }));
    expect(calls("cancel_install")[0]?.[1]).toEqual({ jobId: "job-1" });

    act(() => {
      emitMockEvent(EVENTS.installOutput, {
        jobId: "job-1",
        stream: "stderr",
        line: "npm ERR! ETIMEDOUT",
      });
      emitMockEvent(EVENTS.installDone, {
        jobId: "job-1",
        success: false,
        exitCode: 1,
        durationMs: 10,
        cancelled: false,
      });
    });
    expect(codex).toHaveAttribute("data-phase", "failed");
    expect(within(codex).getByRole("alert")).toHaveTextContent("exit code 1");
    expect(within(codex).getByRole("log")).toHaveTextContent("ETIMEDOUT");
    expect(calls("run_env_check")).toHaveLength(0);
    // the failed job is reported to telemetry (duration only, no free text)
    const tracked = calls("track_event").map((c) => (c[1] as { event: TelemetryEvent }).event);
    expect(tracked).toContainEqual({
      name: "step_result",
      step: "install",
      status: "fail",
      durationMs: 10,
      errorClass: null,
      ruleId: null,
    });

    // the retry re-plans without the registry the job failed on
    fireEvent.click(within(codex).getByRole("button", { name: "Switch mirror and retry" }));
    await waitFor(() => expect(calls("plan_install")).toHaveLength(2));
    expect(calls("plan_install")[1]?.[1]).toEqual({
      target: "codex",
      excludeRegistry: "npmmirror",
    });
    await waitFor(() => expect(codex).toHaveAttribute("data-phase", "confirm"));
  });

  it("npm target: without an alternate registry the retry is a plain retry", async () => {
    useWizardStore
      .getState()
      .setSnapshot(envSnapshot({ codex: checkResult("codex", "fail", { code: "tool.missing" }) }));
    setInvokeHandlers({
      plan_install: () => installPlan("codex"),
      start_install: () => ({ jobId: "job-2", target: "codex" }),
    });
    render(<InstallScreen />);
    const codex = card("codex");
    await waitFor(() => expect(codex).toHaveAttribute("data-phase", "confirm"));
    fireEvent.click(within(codex).getByRole("button", { name: "Confirm and run" }));
    await waitFor(() => expect(codex).toHaveAttribute("data-phase", "running"));
    act(() => {
      emitMockEvent(EVENTS.installDone, {
        jobId: "job-2",
        success: false,
        exitCode: 1,
        durationMs: 5,
        cancelled: false,
      });
    });
    expect(codex).toHaveAttribute("data-phase", "failed");
    expect(within(codex).queryByRole("button", { name: "Switch mirror and retry" })).toBeNull();
    fireEvent.click(within(codex).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(calls("plan_install")).toHaveLength(2));
    expect(calls("plan_install")[1]?.[1]).toEqual({ target: "codex", excludeRegistry: null });
  });

  it("node target: opens the download page and is done once the re-check passes", async () => {
    useWizardStore
      .getState()
      .setSnapshot(envSnapshot({ node: checkResult("node", "fail", { code: "node.missing" }) }));
    setInvokeHandlers({
      plan_install: () =>
        installPlan("node", {
          program: "",
          args: [],
          displayCommand: "",
          explanationCode: "node.download_page",
          registry: {
            id: "official",
            url: "https://nodejs.org/dist/",
            downloadPage: "https://nodejs.org/en/download",
          },
        }),
      run_env_check: () =>
        checkResult("node", "pass", { code: "node.ok", params: { version: "22.1.0" } }),
    });
    render(<InstallScreen />);
    const node = card("node");
    await waitFor(() => expect(node).toHaveAttribute("data-phase", "confirm"));
    expect(within(node).queryByTestId("copy-field-value")).toBeNull();

    fireEvent.click(within(node).getByRole("button", { name: "Open download page" }));
    expect(calls("open_external")[0]?.[1]).toEqual({ url: "https://nodejs.org/en/download" });

    fireEvent.click(within(node).getByRole("button", { name: "I installed it — re-check" }));
    await waitFor(() => expect(node).toHaveAttribute("data-phase", "done"));
    expect(within(node).getByTestId("recheck-feedback")).toHaveTextContent(
      "Node.js 22.1.0 is installed",
    );
    expect(screen.getByRole("button", { name: "Next" })).toBeEnabled();
  });

  it("cc-switch target: release info → download with progress → verified badge → open installer", async () => {
    useWizardStore
      .getState()
      .setSnapshot(
        envSnapshot({ cc_switch: checkResult("cc_switch", "fail", { code: "cc_switch.missing" }) }),
      );
    const release = ccSwitchRelease();
    let downloadArgs: Record<string, unknown> | undefined;
    setInvokeHandlers({
      fetch_cc_switch_release: () => release,
      download_file: (args) => {
        downloadArgs = args;
        return downloadResult();
      },
      run_env_check: () => checkResult("cc_switch", "fail", { code: "cc_switch.missing" }),
    });
    render(<InstallScreen />);
    const cc = card("cc-switch");
    await waitFor(() => expect(cc).toHaveAttribute("data-phase", "release"));
    expect(cc).toHaveTextContent("3.2.0");
    expect(cc).toHaveTextContent(release.assetName);
    expect(cc).toHaveTextContent("SHA-256 provided");

    fireEvent.click(within(cc).getByRole("button", { name: "Download installer" }));
    await waitFor(() => expect(cc).toHaveAttribute("data-phase", "downloaded"));
    const request = downloadArgs?.request as Record<string, unknown>;
    expect(request).toMatchObject({
      url: release.downloadUrl,
      fileName: release.assetName,
      expectedSha256: release.sha256,
    });
    expect(typeof request.jobId).toBe("string");
    expect(within(cc).getByTestId("verified-badge")).toBeInTheDocument();

    fireEvent.click(within(cc).getByRole("button", { name: "Open installer" }));
    expect(calls("open_downloaded_file")[0]?.[1]).toEqual({ path: downloadResult().path });

    // guide fault G is reachable from here: "the installer was blocked" runs the rule engine
    setInvokeHandlers({
      diagnose: () => [
        {
          ruleId: "G",
          severity: "warning",
          code: "os.app_blocked",
          params: { app: "CC Switch" },
          actions: [],
          checklist: ["smartscreen_more_info", "run_anyway"],
        },
      ],
    });
    fireEvent.click(within(cc).getByTestId("installer-blocked"));
    await waitFor(() => expect(within(cc).getByTestId("diagnose-panel")).toBeInTheDocument());
    expect(calls("diagnose")[0]?.[1]).toEqual({
      request: {
        symptoms: [{ kind: "app_blocked_by_os", app: "CC Switch" }],
        snapshot: useWizardStore.getState().snapshot,
      },
    });
    expect(within(cc).getByTestId("diagnose-panel")).toHaveTextContent(
      /Installer blocked by the operating system/,
    );

    // a re-check that does not pass keeps the item open with an explanation
    fireEvent.click(within(cc).getByRole("button", { name: "I installed it — re-check" }));
    await waitFor(() => expect(within(cc).getByTestId("recheck-feedback")).toBeInTheDocument());
    expect(cc).toHaveAttribute("data-phase", "downloaded");
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
  });

  it("cc-switch target: shows download progress from events", async () => {
    useWizardStore
      .getState()
      .setSnapshot(
        envSnapshot({ cc_switch: checkResult("cc_switch", "fail", { code: "cc_switch.missing" }) }),
      );
    let jobId = "";
    setInvokeHandlers({
      fetch_cc_switch_release: () => ccSwitchRelease({ sha256: null }),
      download_file: (args) => {
        jobId = (args?.request as { jobId: string }).jobId;
        return new Promise(() => undefined); // never resolves — stays in downloading
      },
    });
    render(<InstallScreen />);
    const cc = card("cc-switch");
    await waitFor(() => expect(cc).toHaveAttribute("data-phase", "release"));
    expect(cc).toHaveTextContent("ships no SHA-256");
    fireEvent.click(within(cc).getByRole("button", { name: "Download installer" }));
    await waitFor(() => expect(cc).toHaveAttribute("data-phase", "downloading"));
    act(() => {
      emitMockEvent(EVENTS.downloadProgress, { jobId, downloaded: 512 * 1024, total: 1024 * 1024 });
    });
    expect(within(cc).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "50");
    expect(cc).toHaveTextContent("512 KB / 1 MB");
  });

  it("cc-switch target: release fetch failure offers retry and the release page", async () => {
    useWizardStore
      .getState()
      .setSnapshot(
        envSnapshot({ cc_switch: checkResult("cc_switch", "fail", { code: "cc_switch.missing" }) }),
      );
    setInvokeHandlers({ fetch_cc_switch_release: rejectWith(wireError("network")) });
    render(<InstallScreen />);
    const cc = card("cc-switch");
    await waitFor(() => expect(cc).toHaveAttribute("data-phase", "failed"));
    expect(within(cc).getByRole("alert")).toHaveTextContent(
      "Could not fetch the CC Switch release",
    );
    fireEvent.click(within(cc).getByRole("button", { name: "Open release page" }));
    expect(calls("open_external")[0]?.[1]).toEqual({
      url: "https://github.com/farion1231/cc-switch/releases/latest",
    });
    setInvokeHandlers({ fetch_cc_switch_release: () => ccSwitchRelease() });
    fireEvent.click(within(cc).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(cc).toHaveAttribute("data-phase", "release"));
  });

  it("skip: warns, persists in the store, enables Next, and can be undone", async () => {
    useWizardStore
      .getState()
      .setSnapshot(envSnapshot({ codex: checkResult("codex", "fail", { code: "tool.missing" }) }));
    setInvokeHandlers({ plan_install: () => installPlan("codex") });
    render(<InstallScreen />);
    const codex = card("codex");
    await waitFor(() => expect(codex).toHaveAttribute("data-phase", "confirm"));

    fireEvent.click(within(codex).getByRole("button", { name: "Skip" }));
    expect(codex).toHaveAttribute("data-phase", "skipped");
    expect(within(codex).getByRole("alert")).toHaveTextContent("Skipped installing Codex CLI");
    expect(useInstallStore.getState().skipped).toEqual(["codex"]);
    expect(screen.getByRole("button", { name: "Next" })).toBeEnabled();

    fireEvent.click(within(codex).getByRole("button", { name: "Undo skip" }));
    expect(codex).toHaveAttribute("data-phase", "idle");
    expect(useInstallStore.getState().skipped).toEqual([]);
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
  });

  it("unlistens from every channel on unmount", async () => {
    useWizardStore
      .getState()
      .setSnapshot(envSnapshot({ codex: checkResult("codex", "fail", { code: "tool.missing" }) }));
    setInvokeHandlers({ plan_install: () => installPlan("codex") });
    const { unmount } = render(<InstallScreen />);
    await waitFor(() => expect(listenerCount(EVENTS.installDone)).toBe(1));
    unmount();
    expect(listenerCount(EVENTS.installOutput)).toBe(0);
    expect(listenerCount(EVENTS.installDone)).toBe(0);
    expect(listenerCount(EVENTS.downloadProgress)).toBe(0);
  });
});

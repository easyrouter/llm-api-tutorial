import { Globe, RefreshCw, SquareTerminal } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, KeyValueList, StatusBadge, type KeyValueItem } from "@/components/ui";
import { formatDuration } from "@/lib/format";
import type { CliCheck, GatewayCheck, TerminalProcess } from "@/lib/types";

/** CLI section of a verification result: badge, version / path or the redacted output tail. */
export function CliCheckView({ cli }: { cli: CliCheck }) {
  const { t } = useTranslation();
  const items: KeyValueItem[] = [];
  if (cli.version)
    items.push({
      key: "version",
      label: t("verify:results.version"),
      value: cli.version,
      mono: true,
    });
  if (cli.path)
    items.push({ key: "path", label: t("verify:results.path"), value: cli.path, mono: true });
  if (!cli.ok && cli.errorClass) {
    items.push({
      key: "errorClass",
      label: t("verify:results.errorClass"),
      value: t(`verify:errorClass.${cli.errorClass}`),
    });
  }
  return (
    <ResultSection
      icon={<SquareTerminal className="size-4" aria-hidden />}
      title={t("verify:results.cli")}
      status={cli.ok ? "pass" : "fail"}
      statusLabel={cli.ok ? t("verify:results.cliOk") : t("verify:results.cliFail")}
      testId="cli-check"
    >
      <KeyValueList items={items} />
      {!cli.ok && cli.outputTail && (
        <OutputBlock label={t("verify:results.outputTail")} text={cli.outputTail} />
      )}
    </ResultSection>
  );
}

/** Gateway section: badge, HTTP status, latency, error class and the redacted server message. */
export function GatewayCheckView({ gateway }: { gateway: GatewayCheck | null }) {
  const { t } = useTranslation();
  if (!gateway) {
    return (
      <ResultSection
        icon={<Globe className="size-4" aria-hidden />}
        title={t("verify:results.gateway")}
        status="skipped"
        statusLabel={t("verify:results.gatewaySkipped")}
        testId="gateway-check"
      />
    );
  }
  const items: KeyValueItem[] = [];
  if (gateway.httpStatus !== null) {
    items.push({
      key: "status",
      label: t("verify:results.httpStatus"),
      value: String(gateway.httpStatus),
      mono: true,
    });
  }
  if (gateway.latencyMs !== null) {
    items.push({
      key: "latency",
      label: t("verify:results.latency"),
      value: formatDuration(gateway.latencyMs),
    });
  }
  if (!gateway.ok && gateway.errorClass) {
    items.push({
      key: "errorClass",
      label: t("verify:results.errorClass"),
      value: t(`verify:errorClass.${gateway.errorClass}`),
    });
  }
  return (
    <ResultSection
      icon={<Globe className="size-4" aria-hidden />}
      title={t("verify:results.gateway")}
      status={gateway.ok ? "pass" : "fail"}
      statusLabel={gateway.ok ? t("verify:results.gatewayOk") : t("verify:results.gatewayFail")}
      testId="gateway-check"
    >
      <KeyValueList items={items} />
      {gateway.message && (
        <OutputBlock label={t("verify:results.message")} text={gateway.message} />
      )}
    </ResultSection>
  );
}

export interface TerminalsAlertProps {
  terminals: readonly TerminalProcess[];
  onRecheck: () => void;
  rechecking?: boolean;
  /** Set after a manual re-check so the user sees that it happened even when the list is empty. */
  rechecked?: boolean;
}

/** Warning listing terminals that are still running (fault C), with a re-check button. */
export function TerminalsAlert({
  terminals,
  onRecheck,
  rechecking,
  rechecked,
}: TerminalsAlertProps) {
  const { t } = useTranslation();
  const recheckButton = (
    <Button
      variant="secondary"
      size="sm"
      onClick={onRecheck}
      loading={rechecking}
      leftIcon={<RefreshCw className="size-4" aria-hidden />}
    >
      {t("verify:actions.recheckTerminals")}
    </Button>
  );

  if (terminals.length === 0) {
    return (
      <Alert variant="success" actions={recheckButton} data-testid="terminals-alert">
        {t("verify:terminals.none")}
        {rechecked && (
          <span className="ml-1 text-neutral-500">{t("verify:terminals.rechecked")}</span>
        )}
      </Alert>
    );
  }
  return (
    <Alert
      variant="warning"
      title={t("verify:terminals.title", { count: terminals.length })}
      actions={recheckButton}
      data-testid="terminals-alert"
    >
      <p>{t("verify:terminals.body")}</p>
      <ul
        aria-label={t("verify:terminals.listLabel")}
        className="mt-2 flex flex-wrap gap-1.5 font-mono text-xs"
      >
        {terminals.map((p) => (
          <li
            key={p.pid}
            className="rounded border border-neutral-300 bg-white/60 px-1.5 py-0.5 dark:border-neutral-700 dark:bg-neutral-900/60"
          >
            {p.name} <span className="text-neutral-500">#{p.pid}</span>
          </li>
        ))}
      </ul>
    </Alert>
  );
}

function ResultSection({
  icon,
  title,
  status,
  statusLabel,
  testId,
  children,
}: {
  icon: ReactNode;
  title: string;
  status: "pass" | "fail" | "skipped";
  statusLabel: string;
  testId: string;
  children?: ReactNode;
}) {
  return (
    <section
      data-testid={testId}
      data-status={status}
      className="rounded-md border border-neutral-200 p-3 dark:border-neutral-800"
    >
      <header className="flex items-center justify-between gap-3">
        <h3 className="inline-flex items-center gap-2 text-sm font-semibold">
          {icon}
          {title}
        </h3>
        <StatusBadge status={status} label={statusLabel} />
      </header>
      {children && <div className="mt-2 space-y-2">{children}</div>}
    </section>
  );
}

function OutputBlock({ label, text }: { label: string; text: string }) {
  return (
    <div>
      <div className="text-xs text-neutral-500">{label}</div>
      <pre className="mt-1 max-h-40 overflow-auto rounded bg-neutral-100 p-2 font-mono text-xs break-words whitespace-pre-wrap text-neutral-800 dark:bg-neutral-950 dark:text-neutral-200">
        {text}
      </pre>
    </div>
  );
}

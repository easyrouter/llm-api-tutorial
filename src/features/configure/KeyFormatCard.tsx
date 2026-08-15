import { CircleAlert, Eye, EyeOff, Info, ShieldCheck } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, Card, ErrorBanner } from "@/components/ui";
import { useAsync } from "@/hooks";
import { cn } from "@/lib/cn";
import { validateApiKey } from "@/lib/tauri";
import type { KeyIssue, KeyValidation } from "@/lib/types";

import { splitKeyIssues } from "./key-issues";
import { useDebouncedValue } from "./useDebouncedValue";

/**
 * "Key format check" card. The key lives only in this component's state: it is never written
 * to a store, log, telemetry or report, and it disappears when the card unmounts (leaving the
 * tab or the screen). Each debounced change is sent once to `validate_api_key`, which returns
 * issues + trimmed length and never echoes the key back.
 */
export function KeyFormatCard() {
  const { t } = useTranslation();
  const inputId = useId();
  const [key, setKey] = useState("");
  const [visible, setVisible] = useState(false);
  const debouncedKey = useDebouncedValue(key);
  const validation = useAsync(validateApiKey);
  const { run, reset } = validation;

  useEffect(() => {
    if (debouncedKey.length === 0) {
      reset();
      return;
    }
    void run(debouncedKey);
  }, [debouncedKey, run, reset]);

  const clear = () => {
    setKey("");
    reset();
  };

  return (
    <Card title={t("guide:key.title")} description={t("guide:key.description")}>
      <div className="space-y-3">
        <label htmlFor={inputId} className="block text-sm font-medium">
          {t("guide:key.label")}
        </label>
        <div className="flex items-stretch gap-2">
          <input
            id={inputId}
            type={visible ? "text" : "password"}
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={t("guide:key.placeholder")}
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            data-testid="key-input"
            className="focus-visible:ring-brand-500 min-w-0 flex-1 rounded-md border border-neutral-300 bg-white px-3 py-2 font-mono text-sm focus-visible:ring-2 focus-visible:outline-none dark:border-neutral-700 dark:bg-neutral-950"
          />
          <Button
            variant="secondary"
            onClick={() => setVisible((v) => !v)}
            aria-pressed={visible}
            leftIcon={
              visible ? (
                <EyeOff className="size-4" aria-hidden />
              ) : (
                <Eye className="size-4" aria-hidden />
              )
            }
          >
            {visible ? t("guide:key.hide") : t("guide:key.show")}
          </Button>
          <Button variant="ghost" onClick={clear} disabled={key.length === 0}>
            {t("guide:key.clear")}
          </Button>
        </div>
        <p className="flex items-start gap-2 text-xs text-neutral-500">
          <ShieldCheck className="text-success-500 mt-0.5 size-3.5 shrink-0" aria-hidden />
          {t("guide:key.privacy")}
        </p>

        {key.length > 0 && validation.loading && !validation.data && (
          <p className="text-sm text-neutral-500">{t("guide:key.checking")}</p>
        )}
        {key.length > 0 && validation.data && <KeyVerdict validation={validation.data} />}
        {validation.error && (
          <ErrorBanner error={validation.error} onRetry={() => void run(debouncedKey)} />
        )}
      </div>
    </Card>
  );
}

function KeyVerdict({ validation }: { validation: KeyValidation }) {
  const { t } = useTranslation();
  const { blocking, hints } = splitKeyIssues(validation);

  return (
    <div className="space-y-2" data-testid="key-verdict" data-valid={validation.valid}>
      {validation.valid ? (
        <Alert variant="success">{t("guide:key.valid", { length: validation.length })}</Alert>
      ) : (
        <Alert variant="danger" title={t("guide:key.invalid")}>
          <IssueList issues={blocking} kind="blocking" />
        </Alert>
      )}
      {hints.length > 0 && (
        <Alert variant="info" title={t("guide:key.hintLabel")}>
          <IssueList issues={hints} kind="hint" />
        </Alert>
      )}
    </div>
  );
}

function IssueList({ issues, kind }: { issues: KeyIssue[]; kind: "blocking" | "hint" }) {
  const { t } = useTranslation();
  const Icon = kind === "blocking" ? CircleAlert : Info;
  return (
    <ul className="space-y-1">
      {issues.map((issue) => (
        <li
          key={issue}
          data-issue={issue}
          data-kind={kind}
          className={cn("flex items-start gap-2", kind === "blocking" && "font-medium")}
        >
          <Icon
            className={cn(
              "mt-1 size-3.5 shrink-0",
              kind === "blocking" ? "text-danger-500" : "text-brand-600",
            )}
            aria-hidden
          />
          <span>{t(`guide:key.issue.${issue}`)}</span>
        </li>
      ))}
    </ul>
  );
}

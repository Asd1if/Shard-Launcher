import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Copy, ExternalLink, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { DeviceCode } from "@/lib/backend";
import { errorText, isAuthError } from "@/lib/errors";
import { cancelMicrosoftLogin, finishMicrosoftLogin, setAccountsDialogOpen, startMicrosoftLogin } from "./store";

type Step =
  | { kind: "idle" }
  | { kind: "starting" }
  | { kind: "code"; code: DeviceCode; copied: boolean }
  | { kind: "error"; error: unknown };

/** Вход через Microsoft по коду: показываем код, пользователь вводит его на microsoft.com/link. */
export function MicrosoftLogin() {
  const { t } = useTranslation();
  const [step, setStep] = useState<Step>({ kind: "idle" });
  const active = useRef(false);

  // Закрыли окно посреди входа — отменяем ожидание на стороне Rust
  useEffect(
    () => () => {
      if (active.current) void cancelMicrosoftLogin();
    },
    [],
  );

  const start = async () => {
    setStep({ kind: "starting" });
    try {
      const code = await startMicrosoftLogin();
      active.current = true;
      setStep({ kind: "code", code, copied: false });
      await finishMicrosoftLogin();
      active.current = false;
      setStep({ kind: "idle" });
      setAccountsDialogOpen(false);
    } catch (error) {
      active.current = false;
      setStep(isAuthError(error, "cancelled") ? { kind: "idle" } : { kind: "error", error });
    }
  };

  const cancel = () => {
    active.current = false;
    void cancelMicrosoftLogin();
    setStep({ kind: "idle" });
  };

  const copyAndOpen = async (code: DeviceCode) => {
    try {
      await navigator.clipboard.writeText(code.userCode);
      setStep((s) => (s.kind === "code" ? { ...s, copied: true } : s));
    } catch {
      // буфер обмена недоступен — код всё равно виден на экране
    }
    await openUrl(code.verificationUri);
  };

  if (step.kind === "code") {
    const { code, copied } = step;
    return (
      <section className="flex flex-col gap-3 rounded-lg border border-surface-3 bg-surface-0 p-4">
        <p className="text-[13px] text-ink-muted">
          {t("auth.codeStep", { site: code.verificationUri.replace(/^https?:\/\//, "") })}
        </p>
        <div className="flex items-center gap-2">
          <output
            aria-label={t("auth.code")}
            className="pixel-corners flex-1 bg-surface-2 py-3 text-center font-display text-2xl font-bold tracking-[0.2em] select-text [--step:3px]"
          >
            {code.userCode}
          </output>
        </div>
        <Button onClick={() => void copyAndOpen(code)} className="h-10">
          {copied ? <Check /> : <Copy />}
          {t("auth.copyAndOpen")}
          <ExternalLink className="opacity-60" />
        </Button>
        <div className="flex items-center gap-2 text-[13px] text-ink-muted">
          <Loader2 className="size-3.5 animate-spin" />
          <span className="flex-1">{t("auth.waiting")}</span>
          <Button variant="ghost" size="sm" onClick={cancel}>
            {t("create.cancel")}
          </Button>
        </div>
      </section>
    );
  }

  return (
    <section className="flex flex-col gap-2">
      <Button
        variant="secondary"
        className="h-10 justify-start gap-3"
        disabled={step.kind === "starting"}
        onClick={() => void start()}
      >
        {step.kind === "starting" ? <Loader2 className="animate-spin" /> : <MicrosoftMark />}
        {t("auth.signInMicrosoft")}
      </Button>
      {step.kind === "error" && (
        <p role="alert" className="text-xs leading-relaxed text-destructive">
          {errorText(step.error)}
        </p>
      )}
    </section>
  );
}

function MicrosoftMark() {
  return (
    <svg viewBox="0 0 16 16" className="size-4" aria-hidden>
      <rect x="0" y="0" width="7.5" height="7.5" fill="#f25022" />
      <rect x="8.5" y="0" width="7.5" height="7.5" fill="#7fba00" />
      <rect x="0" y="8.5" width="7.5" height="7.5" fill="#00a4ef" />
      <rect x="8.5" y="8.5" width="7.5" height="7.5" fill="#ffb900" />
    </svg>
  );
}

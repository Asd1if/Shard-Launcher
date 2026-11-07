import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { invoke } from "@tauri-apps/api/core";
import { Download, FileText, Loader2, MemoryStick, Play, Puzzle, PowerOff, Coffee, TriangleAlert, Wrench, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { showToast } from "@/components/toast";
import { backend, isTauri, type CrashReport } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { refreshInstances } from "@/features/instances/store";
import { refreshContent } from "@/features/content/store";
import { ShareLogButton } from "@/features/logs/share-log";
import { closeCrash, launch, useCrashDialog, useLaunch } from "./store";

export type FixAction =
  | { type: "install"; id: string; forMods: string[] }
  | { type: "disable"; file: string; name: string }
  | { type: "memory"; from: number; to: number }
  | { type: "java" };

interface FixResult {
  done: string[];
  failed: string[];
}

const fixApi = {
  plan: (id: string, report: CrashReport): Promise<FixAction[]> => {
    if (isTauri) return invoke<FixAction[]>("crash_fix_plan", { id, report });
    // Превью: правдоподобный план по виду ошибки
    if (report.kind === "missing_dependency") return Promise.resolve([{ type: "install", id: "fabric-api", forMods: ["Sodium"] }]);
    if (report.kind === "memory") return Promise.resolve([{ type: "memory", from: 4096, to: 6144 }]);
    return Promise.resolve(report.mods.map((m) => ({ type: "disable" as const, file: `${m.toLowerCase()}.jar`, name: m })));
  },
  apply: (id: string, actions: FixAction[]): Promise<FixResult> =>
    isTauri
      ? invoke<FixResult>("crash_fix_apply", { id, actions })
      : new Promise((r) => setTimeout(() => r({ done: actions.map((a) => ("name" in a ? a.name : "id" in a ? a.id : a.type)), failed: [] }), 900)),
};

/** Окно вылета: причина, что сделает «Починить», лог краша и кнопки «Запустить» и «Починить». */
export function CrashDialog() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const crash = useCrashDialog();
  const report = crash?.report;
  const { logs } = useLaunch(crash?.instanceId ?? "");
  const [plan, setPlan] = useState<FixAction[] | null>(null);
  const [fixing, setFixing] = useState(false);
  const [fixed, setFixed] = useState(false);
  const scroller = useRef<HTMLPreElement>(null);

  useEffect(() => {
    setPlan(null);
    setFixed(false);
    if (!crash) return;
    void fixApi.plan(crash.instanceId, crash.report).then(setPlan, () => setPlan([]));
  }, [crash]);

  // Лог — с конца: там причина
  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [crash, logs.length]);

  if (!crash || !report) return null;

  const relaunch = () => {
    closeCrash();
    void launch(crash.instanceId, crash.name);
  };

  const fix = async () => {
    if (!plan?.length) {
      // Автоматически не починить — ведём туда, где это делается руками
      const tab = ["memory", "java"].includes(report.kind) ? "settings" : "content";
      closeCrash();
      navigate(`/instances/${crash.instanceId}?tab=${tab}`);
      return;
    }
    setFixing(true);
    try {
      const result = await fixApi.apply(crash.instanceId, plan);
      await Promise.all([refreshContent(crash.instanceId), refreshInstances()]);
      if (result.failed.length) showToast(t("crash.fixPartly", { failed: result.failed.join(", ") }));
      else showToast(t("crash.fixDone"));
      setFixed(true);
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setFixing(false);
    }
  };

  const logText = logs.map((l) => l.line).join("\n");
  const canFix = plan === null || plan.length > 0 || ["missing_dependency", "incompatible", "duplicate", "mixin", "mod_crash", "memory", "java"].includes(report.kind);

  return (
    <Dialog open onOpenChange={(open) => !open && closeCrash()}>
      <DialogContent showCloseButton={false} className="flex max-h-[88vh] flex-col gap-4 border-surface-3 bg-surface-1 sm:max-w-[680px]">
        <header className="flex items-start gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-md bg-destructive/15 text-destructive">
            <TriangleAlert className="size-5" />
          </span>
          <div className="min-w-0 flex-1">
            <DialogTitle className="font-display text-lg font-medium">{t("crash.title", { name: crash.name })}</DialogTitle>
            <DialogDescription className="text-[13px]">
              <span className="block font-medium text-foreground">{t(`crash.kind.${report.kind}`)}</span>
              {t(`crash.hint.${report.kind}`, { java: report.javaMajor ?? "?" })}
            </DialogDescription>
          </div>
          <Button variant="ghost" size="icon" aria-label={t("crash.close")} title={t("crash.close")} onClick={closeCrash} className="-mt-1 -mr-2">
            <X />
          </Button>
        </header>

        {report.mods.length > 0 && (
          <ul className="flex flex-wrap gap-1.5">
            {report.mods.map((m) => (
              <li key={m} className="flex items-center gap-1.5 rounded-md bg-surface-2 px-2.5 py-1 text-[13px]">
                <Puzzle className="size-3.5 text-ink-muted" /> {m}
              </li>
            ))}
          </ul>
        )}

        {plan && plan.length > 0 && !fixed && (
          <div className="flex flex-col gap-1.5 rounded-lg border border-brand/30 bg-brand/8 px-4 py-3">
            <p className="text-xs text-ink-muted">{t("crash.planTitle")}</p>
            <ul className="flex flex-col gap-1 text-[13px]">
              {plan.map((a, i) => (
                <li key={i} className="flex items-center gap-2">
                  <PlanIcon action={a} />
                  {a.type === "install"
                    ? a.forMods.length
                      ? t("crash.plan.installFor", { id: a.id, mods: a.forMods.join(", ") })
                      : t("crash.plan.install", { id: a.id })
                    : a.type === "disable"
                      ? t("crash.plan.disable", { name: a.name })
                      : a.type === "memory"
                        ? t("crash.plan.memory", { from: a.from / 1024, to: a.to / 1024 })
                        : t("crash.plan.java")}
                </li>
              ))}
            </ul>
          </div>
        )}
        {fixed && <p className="rounded-lg border border-[#3fb950]/30 bg-[#3fb950]/8 px-4 py-3 text-[13px] text-[#3fb950]">{t("crash.fixedHint")}</p>}

        <div className="flex min-h-0 flex-col gap-1.5">
          <p className="text-xs text-ink-muted">{t("crash.log")}</p>
          <pre
            ref={scroller}
            className="max-h-[38vh] min-h-[140px] overflow-auto rounded-md border border-surface-3 bg-surface-0 p-3 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap text-ink select-text"
          >
            {report.details.length > 0 && <span className="block border-b border-surface-3 pb-2 mb-2 text-destructive">{report.details.join("\n")}</span>}
            {logs.length ? (
              logs.slice(-400).map((l, i) => (
                <span key={i} className={cn("block", l.stderr || /\/(ERROR|FATAL)\]/.test(l.line) ? "text-destructive" : /\/WARN\]/.test(l.line) && "text-[#e0951a]")}>
                  {l.line}
                </span>
              ))
            ) : (
              <span className="text-ink-muted">{t("crash.noLog")}</span>
            )}
          </pre>
        </div>

        <footer className="flex flex-wrap items-center gap-2">
          {report.file && (
            <Button variant="ghost" size="sm" onClick={() => void backend.openCrashReport(report.file!).catch((e) => showToast(errorText(e)))}>
              <FileText /> {t("crash.openReport")}
            </Button>
          )}
          <ShareLogButton content={() => [report.details.join("\n"), logText].filter(Boolean).join("\n\n")} instanceId={crash.instanceId} />
          <span className="flex-1" />
          {canFix && !fixed && (
            <Button variant="secondary" disabled={fixing || plan === null} onClick={() => void fix()}>
              {fixing || plan === null ? <Loader2 className="animate-spin" /> : <Wrench />} {t("crash.fixButton")}
            </Button>
          )}
          <Button onClick={relaunch} disabled={fixing}>
            <Play /> {t("crash.relaunch")}
          </Button>
        </footer>
      </DialogContent>
    </Dialog>
  );
}

function PlanIcon({ action }: { action: FixAction }) {
  const Icon = action.type === "install" ? Download : action.type === "disable" ? PowerOff : action.type === "memory" ? MemoryStick : Coffee;
  return <Icon className="size-3.5 shrink-0 text-ink-muted" />;
}

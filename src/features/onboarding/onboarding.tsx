import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  FolderInput,
  KeyRound,
  Languages,
  Loader2,
  LogIn,
  Palette,
  PartyPopper,
  ScrollText,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ProgressBar, type ByteProgress } from "@/components/ui/progress-bar";
import { showToast } from "@/components/toast";
import { backend, isTauri, type DataDir, type Settings } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { languages, setLanguage, type Language } from "@/i18n";
import { mainThemes, extraThemes, setTheme, themeModes, useThemeChoice } from "@/theme";
import { currentAccount, openAccountsDialog, useAccounts } from "@/features/accounts/store";
import { PlayerHead } from "@/features/accounts/player-head";
import { useAccountFace } from "@/features/accounts/account-face";
import { MemoryPresets } from "@/features/instances/instance-settings";
import { CurseforgeKeyForm } from "@/features/settings/curseforge-key";
import { TERMS_VERSION, TermsText } from "./terms";

const steps = ["language", "terms", "folder", "basics", "account", "curseforge", "done"] as const;
type Step = (typeof steps)[number];

// Шаг переживает перезапуск: перенос папки данных перезапускает лаунчер
const STEP_KEY = "shard.onboarding.step";

function savedStep(): number {
  try {
    const i = Number(localStorage.getItem(STEP_KEY));
    return Number.isInteger(i) && i > 0 && i < steps.length ? i : 0;
  } catch {
    return 0;
  }
}

function saveStep(i: number | null) {
  try {
    if (i === null) localStorage.removeItem(STEP_KEY);
    else localStorage.setItem(STEP_KEY, String(i));
  } catch {
    // не запомнится — начнём сначала
  }
}

const icons: Record<Step, typeof Languages> = {
  language: Languages,
  terms: ScrollText,
  folder: FolderInput,
  basics: Palette,
  account: LogIn,
  curseforge: KeyRound,
  done: PartyPopper,
};

/** Нужен ли мастер первого запуска: настройки не отмечены, и у человека ещё нет ни сборок, ни аккаунтов. */
export function useOnboarding() {
  const [state, setState] = useState<{ show: boolean; settings: Settings } | null>(null);
  useEffect(() => {
    void (async () => {
      const settings = await backend.settings();
      if (settings.onboarded) return setState({ show: false, settings });
      const [instances, accounts] = await Promise.all([backend.instances(), backend.accounts()]);
      // Лаунчером уже пользовались до появления мастера — просто отмечаем
      if (instances.length || accounts.accounts.length) {
        const next = await backend.setSettings({ ...settings, onboarded: true });
        return setState({ show: false, settings: next });
      }
      setState({ show: true, settings });
    })().catch(() => setState({ show: false, settings: {} as Settings }));
  }, []);
  return { loading: state === null, show: !!state?.show, settings: state?.settings, finish: () => setState((s) => s && { ...s, show: false }) };
}

export function Onboarding({ initial, onDone }: { initial: Settings; onDone: () => void }) {
  const { t } = useTranslation();
  const [index, setIndex] = useState(savedStep);
  const [settings, setSettings] = useState(initial);
  const [accepted, setAccepted] = useState(initial.termsVersion >= TERMS_VERSION);
  const [finishing, setFinishing] = useState(false);
  const step = steps[index];

  const go = (i: number) => {
    setIndex(i);
    saveStep(i);
  };

  const save = async (next: Settings) => {
    setSettings(next);
    try {
      setSettings(await backend.setSettings(next));
    } catch (e) {
      showToast(errorText(e));
    }
  };

  const finish = async () => {
    setFinishing(true);
    try {
      await backend.setSettings({ ...settings, onboarded: true, termsVersion: TERMS_VERSION });
      saveStep(null);
      onDone();
    } catch (e) {
      showToast(errorText(e));
      setFinishing(false);
    }
  };

  const next = async () => {
    if (step === "terms") await save({ ...settings, termsVersion: TERMS_VERSION });
    if (step === "done") return finish();
    go(index + 1);
  };

  const canNext = step !== "terms" || accepted;
  const Icon = icons[step];

  return (
    <div className="flex min-h-0 flex-1 items-center justify-center overflow-y-auto px-6 py-8">
      <div className="flex w-full max-w-[620px] flex-col gap-6">
        <ol aria-label={t("onboarding.progress")} className="flex gap-1.5">
          {steps.map((s, i) => (
            <li key={s} className={cn("h-1 flex-1 rounded-full transition-colors", i <= index ? "bg-brand" : "bg-surface-3")} />
          ))}
        </ol>

        <section className="flex flex-col gap-5 rounded-xl border border-surface-3 bg-surface-1/90 p-7 backdrop-blur-md">
          <header className="flex items-start gap-4">
            <span className="pixel-corners grid size-11 shrink-0 place-items-center bg-brand/15 text-brand [--step:3px]">
              <Icon className="size-5" />
            </span>
            <div className="min-w-0">
              <p className="text-xs text-ink-muted">{t("onboarding.stepOf", { step: index + 1, total: steps.length })}</p>
              <h1 className="font-display text-xl font-medium tracking-tight">{t(`onboarding.${step}.title`)}</h1>
              <p className="mt-1 text-sm text-ink-muted">{t(`onboarding.${step}.text`)}</p>
            </div>
          </header>

          {step === "language" && <LanguageStep />}
          {step === "terms" && <TermsStep accepted={accepted} onAccept={setAccepted} />}
          {step === "folder" && <FolderStep onMoving={() => saveStep(index + 1)} />}
          {step === "basics" && <BasicsStep settings={settings} onChange={(s) => void save(s)} />}
          {step === "account" && <AccountStep />}
          {step === "curseforge" && (
            <CurseforgeKeyForm value={settings.curseforgeKey} onSave={(key) => save({ ...settings, curseforgeKey: key })} />
          )}
          {step === "done" && <p className="text-sm text-ink">{t("onboarding.done.more")}</p>}

          <footer className="flex items-center gap-2 border-t border-surface-3 pt-5">
            {index > 0 && (
              <Button variant="ghost" onClick={() => go(index - 1)}>
                <ArrowLeft /> {t("onboarding.back")}
              </Button>
            )}
            <span className="flex-1" />
            {(step === "account" || step === "curseforge" || step === "folder") && (
              <Button variant="ghost" onClick={() => go(index + 1)} className="text-ink-muted">
                {t("onboarding.skip")}
              </Button>
            )}
            <Button disabled={!canNext || finishing} onClick={() => void next()}>
              {finishing && <Loader2 className="animate-spin" />}
              {step === "done" ? t("onboarding.start") : step === "terms" ? t("onboarding.accept") : t("onboarding.next")}
              {step !== "done" && <ArrowRight />}
            </Button>
          </footer>
        </section>
      </div>
    </div>
  );
}

function Choice({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      onClick={onClick}
      className={cn(
        "flex h-10 items-center justify-center gap-2 rounded-md border px-4 text-[13px] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
        active ? "border-brand bg-brand/10 text-foreground" : "border-surface-3 text-ink-muted hover:border-ink-muted hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

function LanguageStep() {
  const { t, i18n } = useTranslation();
  return (
    <div role="radiogroup" aria-label={t("settings.language")} className="grid grid-cols-2 gap-2">
      {languages.map((lang) => (
        <Choice key={lang.code} active={i18n.resolvedLanguage === lang.code} onClick={() => setLanguage(lang.code as Language)}>
          {i18n.resolvedLanguage === lang.code && <Check className="size-4 text-brand" />}
          {lang.label}
        </Choice>
      ))}
    </div>
  );
}

function TermsStep({ accepted, onAccept }: { accepted: boolean; onAccept: (v: boolean) => void }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-3">
      <div className="max-h-[300px] overflow-y-auto rounded-lg border border-surface-3 bg-surface-0 px-5 py-4">
        <TermsText />
      </div>
      <label className="flex cursor-pointer items-center gap-2.5 text-[13px] text-foreground select-none">
        <input type="checkbox" checked={accepted} onChange={(e) => onAccept(e.target.checked)} className="size-4 accent-[var(--brand)]" />
        {t("onboarding.terms.agree")}
      </label>
    </div>
  );
}

function FolderStep({ onMoving }: { onMoving: () => void }) {
  const { t } = useTranslation();
  const [dir, setDir] = useState<DataDir | null>(null);
  const [moving, setMoving] = useState(false);
  const [progress, setProgress] = useState<ByteProgress | null>(null);

  useEffect(() => {
    void backend.dataDir().then(setDir, () => {});
  }, []);

  const pick = async () => {
    if (!isTauri) return showToast(t("onboarding.folder.previewOnly"));
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({ title: t("launcher.dataPick"), directory: true, multiple: false });
    if (typeof path !== "string") return;
    setMoving(true);
    // После переноса лаунчер перезапустится и откроет мастер со следующего шага
    onMoving();
    try {
      await backend.moveData(path, setProgress);
    } catch (e) {
      showToast(errorText(e));
      setMoving(false);
    }
  };

  if (!dir) return <Loader2 className="size-5 animate-spin text-ink-muted" />;
  return (
    <div className="flex flex-col gap-3">
      <code className="truncate rounded-md bg-surface-0 px-3 py-2.5 font-mono text-[12px] select-text" title={dir.path}>
        {dir.path}
      </code>
      {dir.portable ? (
        <p className="text-xs text-ink-muted">{t("launcher.dataPortable")}</p>
      ) : (
        <div>
          <Button variant="secondary" disabled={moving} onClick={() => void pick()}>
            {moving ? <Loader2 className="animate-spin" /> : <FolderInput />} {t("onboarding.folder.change")}
          </Button>
        </div>
      )}
      {moving && <ProgressBar progress={progress} label={t("launcher.dataMoving")} />}
    </div>
  );
}

function BasicsStep({ settings, onChange }: { settings: Settings; onChange: (s: Settings) => void }) {
  const { t } = useTranslation();
  const theme = useThemeChoice();
  const [totalMb, setTotalMb] = useState(8192);
  useEffect(() => {
    void backend.systemInfo().then((s) => setTotalMb(s.totalMemoryMb), () => {});
  }, []);
  const maxMemory = Math.max(1024, Math.floor((totalMb - 2048) / 256) * 256);

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-2">
        <h2 className="text-[13px] text-ink-muted">{t("settings.theme")}</h2>
        <div role="radiogroup" aria-label={t("settings.theme")} className="flex flex-wrap gap-2">
          {[...themeModes, ...mainThemes, ...extraThemes].map((th) => (
            <Choice key={th} active={theme === th} onClick={() => setTheme(th)}>
              {t(`settings.themes.${th}`)}
            </Choice>
          ))}
        </div>
      </div>
      <div className="flex flex-col gap-2">
        <h2 className="text-[13px] text-ink-muted">{t("onboarding.basics.memory")}</h2>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => onChange({ ...settings, defaultMemoryMb: null })}
            className={cn(
              "h-7 rounded-md border px-3 text-xs outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
              settings.defaultMemoryMb === null ? "border-foreground bg-foreground text-background" : "border-surface-3 bg-surface-0 hover:border-ink-muted",
            )}
          >
            {t("launcher.memoryAuto")}
          </button>
          <MemoryPresets value={settings.defaultMemoryMb ?? 0} max={maxMemory} onChange={(mb) => onChange({ ...settings, defaultMemoryMb: mb })} />
        </div>
      </div>
      <label className="flex cursor-pointer items-center justify-between gap-4 text-[13px]">
        <span>
          <span className="block text-foreground">{t("launcher.discord")}</span>
          <span className="block text-xs text-ink-muted">{t("launcher.discordHint")}</span>
        </span>
        <input
          type="checkbox"
          checked={!settings.discordDisabled}
          onChange={(e) => onChange({ ...settings, discordDisabled: !e.target.checked })}
          className="size-4 shrink-0 accent-[var(--brand)]"
        />
      </label>
    </div>
  );
}

function AccountStep() {
  const { t } = useTranslation();
  const account = currentAccount(useAccounts());
  const face = useAccountFace(account);
  return account ? (
    <div className="flex items-center gap-3 rounded-lg border border-surface-3 bg-surface-0 px-4 py-3">
      <PlayerHead name={account.name} face={face} className="size-10" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-foreground">{account.name}</p>
        <p className="text-xs text-ink-muted">{t(`accounts.kind.${account.kind}`)}</p>
      </div>
      <Button variant="ghost" size="sm" onClick={openAccountsDialog}>
        {t("onboarding.account.change")}
      </Button>
    </div>
  ) : (
    <div>
      <Button onClick={openAccountsDialog}>
        <LogIn /> {t("onboarding.account.signIn")}
      </Button>
    </div>
  );
}

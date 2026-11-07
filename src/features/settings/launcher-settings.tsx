import { useEffect, useState } from "react";
import { ProgressBar, type ByteProgress } from "@/components/ui/progress-bar";
import { useTranslation } from "react-i18next";
import { Check, FolderInput, FolderOpen, ImagePlus, Loader2, RotateCcw } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { showToast } from "@/components/toast";
import { Backdrop, backdrops, pickBackdropImage, setBackdrop, useBackdrop, type BackdropKind } from "@/features/backdrop/backdrop";
import { JavaPicker, MemoryPresets } from "@/features/instances/instance-settings";
import { backend, isTauri, type DataDir, type OnLaunch, type Settings } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { CurseforgeKeyForm } from "./curseforge-key";

const radio = (active: boolean) =>
  cn(
    "h-7 rounded px-3 text-[13px] text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
    active && "bg-foreground font-medium text-background hover:text-background",
  );

function Row({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-3 px-5 py-4">
      <div>
        <h3 className="text-foreground">{title}</h3>
        {hint && <p className="text-sm text-ink-muted">{hint}</p>}
      </div>
      {children}
    </div>
  );
}

/** Фон главного экрана: превью вариантов, затемнение, своя картинка. */
export function BackdropSettings() {
  const { t } = useTranslation();
  const prefs = useBackdrop();

  const choose = (kind: BackdropKind) => {
    if (kind === "custom" && !prefs.custom) {
      void pickBackdropImage(t("backdrop.pick")).catch((e) => showToast(errorText(e)));
      return;
    }
    setBackdrop({ kind });
  };

  return (
    <section className="flex flex-col gap-3">
      <div>
        <h2 className="text-foreground">{t("backdrop.title")}</h2>
        <p className="text-sm text-ink-muted">{t("backdrop.hint")}</p>
      </div>
      {/* Ключ перерисовывает превью холстов при смене тона интерфейса */}
      <div key={`${prefs.kind === "custom"}/${prefs.luma}`} role="radiogroup" aria-label={t("backdrop.title")} className="grid grid-cols-5 gap-3">
        {backdrops.map((kind) => {
          const active = prefs.kind === kind;
          return (
            <button
              key={kind}
              type="button"
              role="radio"
              aria-checked={active}
              onClick={() => choose(kind)}
              className={cn(
                "flex flex-col gap-2 rounded-lg border bg-surface-1 p-2 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
                active ? "border-ink-muted" : "border-surface-3 hover:border-ink-muted/50",
              )}
            >
              <span className="relative block h-16 overflow-hidden rounded-md border border-surface-3 bg-background">
                {kind === "custom" && !prefs.custom ? (
                  <span className="grid size-full place-items-center text-ink-muted">
                    <ImagePlus className="size-5" />
                  </span>
                ) : (
                  <Backdrop kind={kind} dim={0} custom={prefs.custom} luma={null} />
                )}
              </span>
              <span className="flex items-center justify-between px-0.5 text-[13px]">
                {t(`backdrop.kinds.${kind}`)}
                {active && <Check className="size-4 text-foreground" />}
              </span>
            </button>
          );
        })}
      </div>
      {prefs.kind !== "none" && (
        <div className="flex flex-wrap items-center gap-4">
          <label className="flex flex-1 items-center gap-3 text-[13px] text-ink-muted">
            {t("backdrop.dim")}
            <input
              type="range"
              min={0}
              max={80}
              step={5}
              value={prefs.dim}
              onChange={(e) => setBackdrop({ dim: Number(e.target.value) })}
              className="h-1.5 max-w-[240px] flex-1 cursor-pointer accent-[var(--brand)]"
            />
            <output className="w-10 tabular-nums">{prefs.dim}%</output>
          </label>
          {prefs.kind === "custom" && (
            <Button variant="ghost" size="sm" onClick={() => void pickBackdropImage(t("backdrop.pick")).catch((e) => showToast(errorText(e)))}>
              <ImagePlus /> {t("backdrop.change")}
            </Button>
          )}
        </div>
      )}
    </section>
  );
}

/** Значения по умолчанию для сборок, поведение окна при запуске, Discord и папка с данными. */
export function LauncherSettings() {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState<Settings | null>(null);
  const [totalMb, setTotalMb] = useState(8192);
  const [dataDir, setDataDir] = useState<DataDir | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void backend.settings().then((s) => {
      setSettings(s);
      setSaved(s);
    });
    void backend.systemInfo().then((s) => setTotalMb(s.totalMemoryMb));
    void backend.dataDir().then(setDataDir);
  }, []);

  if (!settings) return null;
  const maxMemory = Math.max(1024, Math.floor((totalMb - 2048) / 256) * 256);
  const dirty = JSON.stringify(settings) !== JSON.stringify(saved);
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => setSettings((s) => (s ? { ...s, [key]: value } : s));

  // Поведение окна и Discord сохраняем сразу — это переключатели, а не форма
  // Сохраняется только переключённое поле: несохранённые правки формы не трогаем,
  // а «сохранённое» обновляем сразу — иначе на миг мелькает панель «Сохранить»
  const saveNow = async <K extends keyof Settings>(key: K, value: Settings[K]) => {
    if (!saved) return;
    const before = saved;
    const next = { ...saved, [key]: value };
    setSettings((s) => (s ? { ...s, [key]: value } : s));
    setSaved(next);
    try {
      await backend.setSettings(next);
    } catch (e) {
      setSettings((s) => (s ? { ...s, [key]: before[key] } : s));
      setSaved(before);
      showToast(errorText(e));
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      const result = await backend.setSettings(settings);
      setSettings(result);
      setSaved(result);
      showToast(t("settings.saved"));
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      <section className="flex flex-col gap-3">
        <h2 className="text-foreground">{t("launcher.defaultsTitle")}</h2>
        <div className="flex flex-col divide-y divide-surface-3 rounded-lg border border-surface-3 bg-surface-1">
          <Row title={t("launcher.memory")} hint={t("launcher.memoryHint")}>
            <div className="flex flex-wrap items-center gap-2">
              <button type="button" className={cn(radio(settings.defaultMemoryMb === null), "border border-surface-3")} onClick={() => set("defaultMemoryMb", null)}>
                {t("launcher.memoryAuto")}
              </button>
              <MemoryPresets value={settings.defaultMemoryMb ?? 0} max={maxMemory} onChange={(mb) => set("defaultMemoryMb", mb)} />
            </div>
          </Row>
          <Row title={t("launcher.java")} hint={t("launcher.javaHint")}>
            <JavaPicker value={settings.defaultJavaPath} onChange={(path) => set("defaultJavaPath", path)} />
          </Row>
          <Row title={t("launcher.jvmArgs")} hint={t("launcher.jvmArgsHint")}>
            <Input
              value={settings.defaultJvmArgs}
              onChange={(e) => set("defaultJvmArgs", e.target.value)}
              placeholder="-XX:+UseG1GC"
              className="h-9 bg-surface-0 font-mono text-[12px]"
            />
          </Row>
          {dirty && (
            <div className="flex justify-end gap-2 px-5 py-3">
              <Button variant="ghost" size="sm" onClick={() => setSettings(saved)}>
                {t("create.cancel")}
              </Button>
              <Button size="sm" disabled={saving} onClick={() => void save()}>
                {saving && <Loader2 className="animate-spin" />} {t("settings.save")}
              </Button>
            </div>
          )}
        </div>
      </section>

      <section className="flex flex-col divide-y divide-surface-3 rounded-lg border border-surface-3 bg-surface-1">
        <div className="flex items-center justify-between gap-6 px-5 py-4">
          <div>
            <h2 className="text-foreground">{t("launcher.onLaunch")}</h2>
            <p className="text-sm text-ink-muted">{t("launcher.onLaunchHint")}</p>
          </div>
          <div role="radiogroup" aria-label={t("launcher.onLaunch")} className="flex shrink-0 rounded-md bg-surface-2 p-1">
            {(["keep", "minimize", "close"] as OnLaunch[]).map((v) => (
              <button
                key={v}
                type="button"
                role="radio"
                aria-checked={settings.onLaunch === v}
                onClick={() => void saveNow("onLaunch", v)}
                className={radio(settings.onLaunch === v)}
              >
                {t(`launcher.onLaunchOptions.${v}`)}
              </button>
            ))}
          </div>
        </div>
        <label className="flex cursor-pointer items-center justify-between gap-6 px-5 py-4">
          <div>
            <h2 className="text-foreground">{t("launcher.discord")}</h2>
            <p className="text-sm text-ink-muted">{t("launcher.discordHint")}</p>
          </div>
          <input
            type="checkbox"
            checked={!settings.discordDisabled}
            onChange={(e) => void saveNow("discordDisabled", !e.target.checked)}
            className="size-4 shrink-0 accent-[var(--brand)]"
          />
        </label>
        <label className="flex cursor-pointer items-center justify-between gap-6 px-5 py-4">
          <div>
            <h2 className="text-foreground">{t("launcher.news")}</h2>
            <p className="text-sm text-ink-muted">{t("launcher.newsHint")}</p>
          </div>
          <input
            type="checkbox"
            checked={!settings.hideNews}
            onChange={(e) => void saveNow("hideNews", !e.target.checked)}
            className="size-4 shrink-0 accent-[var(--brand)]"
          />
        </label>
        {dataDir && <DataDirRow dir={dataDir} />}
        <div className="flex flex-col gap-3 px-5 py-4">
          <div>
            <h2 className="text-foreground">{t("cfKey.title")}</h2>
            <p className="text-sm text-ink-muted">{t("cfKey.hint")}</p>
          </div>
          <CurseforgeKeyForm value={settings.curseforgeKey} onSave={(key) => saveNow("curseforgeKey", key)} />
        </div>
      </section>
    </>
  );
}

function DataDirRow({ dir }: { dir: DataDir }) {
  const { t } = useTranslation();
  const [target, setTarget] = useState<string | null | undefined>(undefined);
  const [moving, setMoving] = useState(false);
  const [progress, setProgress] = useState<ByteProgress | null>(null);

  const pick = async () => {
    if (!isTauri) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({ title: t("launcher.dataPick"), directory: true, multiple: false });
    if (typeof path === "string") setTarget(path);
  };

  const move = async () => {
    setMoving(true);
    setProgress(null);
    try {
      // Лаунчер перезапустится сам, ответа не будет
      await backend.moveData(target ?? null, setProgress);
    } catch (e) {
      showToast(errorText(e));
      setMoving(false);
    }
  };

  return (
    <div className="flex flex-col gap-3 px-5 py-4">
      <div>
        <h2 className="text-foreground">{t("launcher.dataDir")}</h2>
        <p className="text-sm text-ink-muted">{dir.portable ? t("launcher.dataPortable") : t("launcher.dataHint")}</p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <code className="min-w-0 flex-1 truncate rounded-md bg-surface-0 px-3 py-2 font-mono text-[12px] select-text" title={dir.path}>
          {dir.path}
        </code>
        <Button variant="ghost" size="icon" title={t("launcher.dataOpen")} aria-label={t("launcher.dataOpen")} onClick={() => isTauri && void revealItemInDir(dir.path)}>
          <FolderOpen />
        </Button>
        {!dir.portable && (
          <>
            <Button variant="secondary" size="sm" onClick={() => void pick()}>
              <FolderInput /> {t("launcher.dataChange")}
            </Button>
            {!dir.isDefault && (
              <Button variant="ghost" size="sm" onClick={() => setTarget(null)}>
                <RotateCcw /> {t("launcher.dataReset")}
              </Button>
            )}
          </>
        )}
      </div>

      <Dialog open={target !== undefined} onOpenChange={(o) => !o && !moving && setTarget(undefined)}>
        <DialogContent className="border-surface-3 bg-surface-1 sm:max-w-[460px]">
          <DialogHeader>
            <DialogTitle>{t("launcher.dataMoveTitle")}</DialogTitle>
            <DialogDescription>{t("launcher.dataMoveText", { path: target ?? t("launcher.dataDefault") })}</DialogDescription>
          </DialogHeader>
          {moving && <ProgressBar progress={progress} label={t("launcher.dataMoving")} />}
          <DialogFooter>
            <Button variant="ghost" disabled={moving} onClick={() => setTarget(undefined)}>
              {t("create.cancel")}
            </Button>
            <Button disabled={moving} onClick={() => void move()}>
              {moving && <Loader2 className="animate-spin" />} {moving ? t("launcher.dataMoving") : t("launcher.dataMove")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

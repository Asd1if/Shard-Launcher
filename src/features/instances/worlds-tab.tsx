import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Archive, ChevronLeft, ChevronRight, FolderOpen, Globe2, History, ImageOff, Loader2, RotateCcw, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/page";
import { showToast } from "@/components/toast";
import { backend, isTauri, type Backup, type Screenshot, type Settings, type World } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { timeAgo } from "@/lib/time";
import { formatSize } from "@/lib/format";
import { ProgressBar, type ByteProgress } from "@/components/ui/progress-bar";
import type { Instance } from "./types";

/** Картинка с диска через asset-протокол Tauri. */
const fileSrc = (path: string) => (isTauri ? convertFileSrc(path) : path);

const open = (id: string, relative: string, reveal = false) =>
  void backend.openPath(id, relative, reveal).catch((e) => showToast(errorText(e)));

/** Миры сборки: дата, размер, резервная копия одной кнопкой. */
export function WorldsTab({ instance }: { instance: Instance }) {
  const { t, i18n } = useTranslation();
  const [worlds, setWorlds] = useState<World[] | null>(null);
  const [backups, setBackups] = useState<Backup[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [progress, setProgress] = useState<ByteProgress | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);

  useEffect(() => void backend.settings().then(setSettings, () => {}), []);
  const toggleAuto = async (on: boolean) => {
    if (!settings) return;
    try {
      setSettings(await backend.setSettings({ ...settings, autoBackup: on }));
    } catch (e) {
      showToast(errorText(e));
    }
  };

  const restore = async (b: Backup) => {
    setConfirm(null);
    setBusy(b.file);
    try {
      await backend.restoreWorld(instance.id, b.file);
      showToast(t("worlds.restored", { world: b.world }));
      load();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const removeBackup = async (b: Backup) => {
    try {
      await backend.deleteBackup(instance.id, b.file);
      load();
    } catch (e) {
      showToast(errorText(e));
    }
  };

  const load = () => {
    backend.worlds(instance.id).then(setWorlds, (e) => {
      setWorlds([]);
      showToast(errorText(e));
    });
    backend.backups(instance.id).then(setBackups, () => setBackups([]));
  };
  useEffect(load, [instance.id]);

  const backup = async (world: string) => {
    setBusy(world);
    setProgress(null);
    try {
      const b = await backend.backupWorld(instance.id, world, setProgress);
      showToast(t("worlds.backedUp", { world, size: formatSize(b.size, i18n.language) }));
      load();
    } catch (e) {
      showToast(t("worlds.backupFailed", { error: errorText(e) }));
    } finally {
      setBusy(null);
    }
  };

  if (worlds === null) {
    return (
      <p className="flex items-center gap-2 py-6 text-[13px] text-ink-muted">
        <Loader2 className="size-4 animate-spin" /> {t("worlds.loading")}
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-3 pb-6">
      <div className="flex flex-wrap items-center gap-2">
        <p className="text-xs text-ink-muted">{t("worlds.count", { count: worlds.length })}</p>
        <span className="flex-1" />
        {settings && (
          <label className="flex cursor-pointer items-center gap-2 text-xs text-ink-muted select-none hover:text-foreground" title={t("worlds.autoBackupHint")}>
            <input type="checkbox" checked={settings.autoBackup} onChange={(e) => void toggleAuto(e.target.checked)} />
            {t("worlds.autoBackup")}
          </label>
        )}
        <Button variant="ghost" size="sm" onClick={() => open(instance.id, "@/backups", true)}>
          <Archive /> {t("worlds.backupsFolder", { count: backups.length })}
        </Button>
        <Button variant="ghost" size="sm" onClick={() => open(instance.id, "saves", true)}>
          <FolderOpen /> {t("worlds.openFolder")}
        </Button>
      </div>

      {worlds.length === 0 ? (
        <EmptyState icon={<Globe2 className="size-7" />} title={t("worlds.emptyTitle")} text={t("worlds.emptyText")} />
      ) : (
        <ul className="flex flex-col divide-y divide-surface-3 rounded-lg border border-surface-3 bg-surface-1">
          {worlds.map((w) => {
            const last = backups.find((b) => b.world === w.folder);
            return (
              <li key={w.folder} className="flex items-center gap-4 px-4 py-3">
                {w.icon ? (
                  <img src={fileSrc(w.icon)} alt="" className="size-11 rounded-md [image-rendering:pixelated]" />
                ) : (
                  <span className="grid size-11 place-items-center rounded-md bg-surface-2 text-ink-muted">
                    <Globe2 className="size-5" />
                  </span>
                )}
                <div className="min-w-0 flex-1">
                  <p className="truncate text-foreground">{w.folder}</p>
                  {busy === w.folder ? (
                    <ProgressBar progress={progress} className="mt-1.5" />
                  ) : (
                    <p className="truncate text-xs text-ink-muted">
                      {[
                        w.lastPlayed ? t("worlds.played", { when: timeAgo(w.lastPlayed, i18n.language) }) : null,
                        formatSize(w.size, i18n.language),
                        last ? t("worlds.lastBackup", { when: timeAgo(last.createdAt, i18n.language) }) : t("worlds.noBackup"),
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                    </p>
                  )}
                </div>
                <Button
                  variant="ghost"
                  size="icon"
                  title={t("worlds.open")}
                  aria-label={t("worlds.open")}
                  onClick={() => open(instance.id, `saves/${w.folder}`)}
                >
                  <FolderOpen />
                </Button>
                <Button size="sm" variant="secondary" disabled={!!busy} onClick={() => void backup(w.folder)} className="w-[150px]">
                  {busy === w.folder ? <Loader2 className="animate-spin" /> : <Archive />}
                  {busy === w.folder ? t("worlds.backingUp") : t("worlds.backup")}
                </Button>
              </li>
            );
          })}
        </ul>
      )}

      {backups.length > 0 && (
        <section className="flex flex-col gap-2 pt-2">
          <h3 className="flex items-center gap-2 text-[13px] text-ink-muted">
            <History className="size-4" /> {t("worlds.backupsTitle")}
          </h3>
          <ul className="flex flex-col divide-y divide-surface-3 rounded-lg border border-surface-3 bg-surface-1">
            {backups.map((b) => (
              <li key={b.file} className="flex items-center gap-3 px-4 py-2">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13px] text-foreground">{b.world}</p>
                  <p className="truncate text-xs text-ink-muted">
                    {new Date(b.createdAt).toLocaleString(i18n.language)} · {formatSize(b.size, i18n.language)}
                  </p>
                </div>
                {confirm === b.file ? (
                  <>
                    <span className="text-xs text-ink-muted">{t("worlds.restoreConfirm")}</span>
                    <Button size="sm" variant="secondary" onClick={() => void restore(b)}>
                      {t("worlds.restore")}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setConfirm(null)}>
                      {t("files.cancel")}
                    </Button>
                  </>
                ) : (
                  <>
                    <Button size="sm" variant="ghost" disabled={!!busy} onClick={() => setConfirm(b.file)}>
                      {busy === b.file ? <Loader2 className="animate-spin" /> : <RotateCcw />} {t("worlds.restore")}
                    </Button>
                    <Button size="icon" variant="ghost" disabled={!!busy} title={t("files.delete")} aria-label={t("files.delete")} onClick={() => void removeBackup(b)}>
                      <Trash2 />
                    </Button>
                  </>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

/** Скриншоты сборки: сетка миниатюр и просмотр по клику. */
export function ScreenshotsTab({ instance }: { instance: Instance }) {
  const { t, i18n } = useTranslation();
  const [shots, setShots] = useState<Screenshot[] | null>(null);
  const [viewing, setViewing] = useState<number | null>(null);

  useEffect(() => {
    backend.screenshots(instance.id).then(setShots, () => setShots([]));
  }, [instance.id]);

  useEffect(() => {
    if (viewing === null || !shots) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setViewing(null);
      if (e.key === "ArrowRight") setViewing((v) => (v === null ? v : Math.min(shots.length - 1, v + 1)));
      if (e.key === "ArrowLeft") setViewing((v) => (v === null ? v : Math.max(0, v - 1)));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [viewing, shots]);

  if (shots === null) {
    return (
      <p className="flex items-center gap-2 py-6 text-[13px] text-ink-muted">
        <Loader2 className="size-4 animate-spin" /> {t("worlds.loading")}
      </p>
    );
  }
  const current = viewing === null ? null : shots[viewing];

  return (
    <div className="flex flex-col gap-3 pb-6">
      <div className="flex items-center gap-2">
        <p className="text-xs text-ink-muted">{t("screenshots.count", { count: shots.length })}</p>
        <span className="flex-1" />
        <Button variant="ghost" size="sm" onClick={() => open(instance.id, "screenshots", true)}>
          <FolderOpen /> {t("worlds.openFolder")}
        </Button>
      </div>

      {shots.length === 0 ? (
        <EmptyState icon={<ImageOff className="size-7" />} title={t("screenshots.emptyTitle")} text={t("screenshots.emptyText")} />
      ) : (
        <ul className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-2">
          {shots.map((s, i) => (
            <li key={s.file}>
              <button
                type="button"
                onClick={() => setViewing(i)}
                className="group relative block aspect-video w-full overflow-hidden rounded-lg border border-surface-3 bg-surface-2 outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <img
                  src={fileSrc(s.path)}
                  alt={s.file}
                  loading="lazy"
                  className="size-full object-cover transition-transform group-hover:scale-[1.03]"
                />
                <span className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/70 to-transparent px-2 pt-4 pb-1.5 text-left text-[11px] text-white/90">
                  {timeAgo(s.takenAt, i18n.language)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}

      {current && viewing !== null && (
        <div role="dialog" aria-modal className="fixed inset-0 z-50 flex flex-col bg-black/90" onClick={() => setViewing(null)}>
          <div className="flex items-center gap-3 px-4 py-3 text-[13px] text-white/80" onClick={(e) => e.stopPropagation()}>
            <span className="truncate">{current.file}</span>
            <span className="text-white/50">
              {formatSize(current.size, i18n.language)} · {viewing + 1} / {shots.length}
            </span>
            <span className="flex-1" />
            <Button
              variant="ghost"
              size="sm"
              className="text-white/80 hover:text-white"
              onClick={() => open(instance.id, `screenshots/${current.file}`)}
            >
              {t("screenshots.open")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="text-white/80 hover:text-white"
              onClick={() => open(instance.id, `screenshots/${current.file}`, true)}
            >
              <FolderOpen /> {t("screenshots.reveal")}
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="text-white/80 hover:text-white"
              aria-label={t("icon.close")}
              onClick={() => setViewing(null)}
            >
              <X />
            </Button>
          </div>
          <div className="relative flex min-h-0 flex-1 items-center justify-center px-14 pb-6">
            <img
              src={fileSrc(current.path)}
              alt={current.file}
              className="max-h-full max-w-full rounded object-contain"
              onClick={(e) => e.stopPropagation()}
            />
            {viewing > 0 && (
              <button
                type="button"
                aria-label={t("screenshots.prev")}
                onClick={(e) => {
                  e.stopPropagation();
                  setViewing(viewing - 1);
                }}
                className="absolute left-3 grid size-10 place-items-center rounded-full bg-white/10 text-white hover:bg-white/20"
              >
                <ChevronLeft />
              </button>
            )}
            {viewing < shots.length - 1 && (
              <button
                type="button"
                aria-label={t("screenshots.next")}
                onClick={(e) => {
                  e.stopPropagation();
                  setViewing(viewing + 1);
                }}
                className="absolute right-3 grid size-10 place-items-center rounded-full bg-white/10 text-white hover:bg-white/20"
              >
                <ChevronRight />
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

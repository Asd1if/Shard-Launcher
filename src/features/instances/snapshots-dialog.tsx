import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Channel, invoke } from "@tauri-apps/api/core";
import { Camera, Loader2, RotateCcw, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { showToast } from "@/components/toast";
import { ProgressBar } from "@/components/ui/progress-bar";
import { refreshContent } from "@/features/content/store";
import { isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { formatSize } from "@/lib/format";
import { refreshInstances } from "./store";

export interface Snapshot {
  file: string;
  reason: "manual" | "update" | "pack" | "restore";
  createdAt: string;
  size: number;
  mods: number;
}

const preview: Snapshot[] = [
  { file: "a.zip", reason: "update", createdAt: new Date().toISOString(), size: 48_000_000, mods: 112 },
  { file: "b.zip", reason: "manual", createdAt: new Date(Date.now() - 86_400_000 * 3).toISOString(), size: 46_500_000, mods: 108 },
];

/** Прогресс снимка (save) и отката (restore) в байтах. */
export interface SnapProgress {
  stage: "save" | "restore";
  done: number;
  total: number;
}

function progressChannel(onProgress: (p: SnapProgress) => void) {
  const ch = new Channel<SnapProgress>();
  ch.onmessage = onProgress;
  return ch;
}

/** Превью без Rust: полоска пробегает этапы, чтобы было видно, как она выглядит. */
async function fakeProgress(onProgress: (p: SnapProgress) => void, stages: SnapProgress["stage"][]) {
  const total = 48_000_000;
  for (const stage of stages) {
    for (let step = 0; step <= 8; step++) {
      onProgress({ stage, done: (total * step) / 8, total });
      await new Promise((r) => setTimeout(r, 120));
    }
  }
}

/** Снимок перед обновлением модов — тихо, ошибка не мешает обновлению. */
export async function snapshotBeforeUpdate(id: string, onProgress?: (p: SnapProgress) => void) {
  if (!isTauri) return;
  try {
    await invoke("snapshot_create", { id, reason: "update", progress: progressChannel(onProgress ?? (() => {})) });
  } catch {
    // без снимка обновление всё равно идёт
  }
}

/** Снимки сборки: сделать, откатиться, удалить. */
export function SnapshotsDialog({ id, name, onOpenChange }: { id: string; name: string; onOpenChange: (open: boolean) => void }) {
  const { t, i18n } = useTranslation();
  const [list, setList] = useState<Snapshot[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [progress, setProgress] = useState<SnapProgress | null>(null);

  useEffect(() => {
    if (!isTauri) return setList(preview);
    void invoke<Snapshot[]>("snapshots", { id }).then(setList, () => setList([]));
  }, [id]);

  const run = async (key: string, fn: () => Promise<void>) => {
    setBusy(key);
    setProgress(null);
    try {
      await fn();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };
  const create = () =>
    run("create", async () => {
      if (!isTauri) return fakeProgress(setProgress, ["save"]);
      setList(await invoke<Snapshot[]>("snapshot_create", { id, progress: progressChannel(setProgress) }));
    });
  const remove = (file: string) =>
    run(file, async () => setList(isTauri ? await invoke<Snapshot[]>("snapshot_delete", { id, file }) : (list ?? []).filter((s) => s.file !== file)));
  const restore = (file: string) =>
    run(file, async () => {
      if (isTauri) await invoke("snapshot_restore", { id, file, progress: progressChannel(setProgress) });
      else await fakeProgress(setProgress, ["save", "restore"]);
      await Promise.all([refreshInstances(), refreshContent(id)]);
      if (isTauri) setList(await invoke<Snapshot[]>("snapshots", { id }));
      setConfirm(null);
      showToast(t("snapshots.restored"));
    });

  return (
    <Dialog open onOpenChange={(o) => !busy && onOpenChange(o)}>
      <DialogContent className="border-surface-3 bg-surface-1 sm:max-w-[520px]">
        <DialogHeader>
          <DialogTitle>{t("snapshots.title")}</DialogTitle>
          <DialogDescription>{t("snapshots.text", { name })}</DialogDescription>
        </DialogHeader>
        <Button variant="secondary" className="self-start" disabled={!!busy} onClick={() => void create()}>
          {busy === "create" ? <Loader2 className="animate-spin" /> : <Camera />} {t("snapshots.create")}
        </Button>
        {busy && (
          <ProgressBar
            progress={progress}
            label={busy === "create" ? t("snapshots.progress.save") : progress?.stage === "restore" ? t("snapshots.progress.restore") : t("snapshots.progress.backup")}
          />
        )}
        <div className="flex max-h-80 flex-col gap-1 overflow-y-auto">
          {list === null ? (
            <Loader2 className="mx-auto my-4 animate-spin text-ink-muted" />
          ) : list.length === 0 ? (
            <p className="py-3 text-[13px] text-ink-muted">{t("snapshots.empty")}</p>
          ) : (
            list.map((s) => (
              <div key={s.file} className="flex items-center gap-3 rounded-md border border-surface-3 bg-surface-0 px-3 py-2">
                <div className="min-w-0 flex-1">
                  <p className="text-[13px] text-foreground">
                    {new Date(s.createdAt).toLocaleString(i18n.language, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}
                    <span className="ml-2 rounded bg-surface-2 px-1.5 py-px text-[11px] text-ink-muted">{t(`snapshots.reason.${s.reason}`)}</span>
                  </p>
                  <p className="text-xs text-ink-muted">
                    {t("snapshots.mods", { count: s.mods })} · {formatSize(s.size, i18n.language)}
                  </p>
                </div>
                {confirm === s.file ? (
                  <>
                    <Button size="sm" variant="ghost" onClick={() => setConfirm(null)}>
                      {t("create.cancel")}
                    </Button>
                    <Button size="sm" disabled={!!busy} onClick={() => void restore(s.file)}>
                      {busy === s.file && <Loader2 className="animate-spin" />} {t("snapshots.confirm")}
                    </Button>
                  </>
                ) : (
                  <>
                    <Button size="sm" variant="ghost" disabled={!!busy} onClick={() => setConfirm(s.file)}>
                      <RotateCcw /> {t("snapshots.restore")}
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="size-8 hover:text-destructive"
                      disabled={!!busy}
                      onClick={() => void remove(s.file)}
                      aria-label={t("snapshots.delete")}
                      title={t("snapshots.delete")}
                    >
                      <Trash2 />
                    </Button>
                  </>
                )}
              </div>
            ))
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

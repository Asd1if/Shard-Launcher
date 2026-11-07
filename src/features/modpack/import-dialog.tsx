import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { FileArchive, Layers, Link2, Loader2 } from "lucide-react";
import { LauncherImport } from "./launcher-import";
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
import { refreshInstances } from "@/features/instances/store";
import { openManual, refreshContent } from "@/features/content/store";
import { backend, isTauri } from "@/lib/backend";
import i18n from "@/i18n";
import { errorText } from "@/lib/errors";
import { createStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { isPackFile, modpackApi, pickPackFile, type ImportSource, type PackProgress } from "./api";

/** Открыт ли диалог; source — архив, который уже перетащили в окно. */
const store = createStore<{ open: boolean; source: ImportSource | null }>({ open: false, source: null });

export const openImport = (source: ImportSource | null = null) => store.set({ open: true, source });

/** Открытая сейчас сборка: файлы, брошенные в окно, попадают в неё. */
const dropTarget = createStore<{ id: string; name: string } | null>(null);
export const setDropTarget = dropTarget.set;

const isDroppable = (path: string) => /\.(mrpack|zip|jar)$/i.test(path);

/** Моды, ресурспаки и шейдеры — в открытую сборку; архивы модпаков — в импорт. */
async function dropIntoInstance(id: string, name: string, paths: string[]) {
  try {
    const result = await backend.addFiles(id, paths);
    if (result.added.length) {
      showToast(
        result.added.length === 1
          ? i18n.t("drop.addedOne", { file: result.added[0][0], name })
          : i18n.t("drop.added", { count: result.added.length, name }),
      );
      await Promise.all([refreshContent(id), refreshInstances()]);
    }
    if (result.skipped.length) showToast(i18n.t("drop.skipped", { files: result.skipped.join(", ") }));
    if (result.packs.length) openImport({ path: result.packs[0] });
  } catch (e) {
    showToast(errorText(e));
  }
}

/** Перетаскивание файлов в окно: на странице сборки — в неё, иначе .mrpack или .zip запускает импорт. */
function useDropImport() {
  const [over, setOver] = useState<"pack" | "instance" | null>(null);
  useEffect(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent((e) => {
          const p = e.payload;
          const target = dropTarget.get();
          if (p.type === "enter") {
            setOver(target ? (p.paths.some(isDroppable) ? "instance" : null) : p.paths.some(isPackFile) ? "pack" : null);
          } else if (p.type === "leave") setOver(null);
          else if (p.type === "drop") {
            setOver(null);
            if (target) {
              const paths = p.paths.filter(isDroppable);
              if (paths.length) void dropIntoInstance(target.id, target.name, paths);
              return;
            }
            const path = p.paths.find(isPackFile);
            if (path) openImport({ path });
          }
        })
        .then((fn) => (cancelled ? fn() : (unlisten = fn))),
    );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
  return over;
}

export function ImportDialog() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { open, source } = store.use();
  const [url, setUrl] = useState("");
  const [progress, setProgress] = useState<PackProgress | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [launchers, setLaunchers] = useState(false);
  const dragging = useDropImport();
  const target = dropTarget.use();

  const close = () => {
    if (running) return;
    store.set({ open: false, source: null });
    setError(null);
    setProgress(null);
    setUrl("");
    setLaunchers(false);
  };

  const run = async (src: ImportSource) => {
    setRunning(true);
    setError(null);
    setProgress(null);
    try {
      const result = await modpackApi.import(src, setProgress);
      await refreshInstances();
      store.set({ open: false, source: null });
      setUrl("");
      showToast(t("modpack.imported", { name: result.instance.name }));
      navigate(`/instances/${result.instance.id}`);
      if (result.manual.length) openManual(result.instance.id, result.manual);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setRunning(false);
      setProgress(null);
    }
  };

  // Архив, перетащенный в окно, импортируем сразу
  useEffect(() => {
    if (open && source && !running) void run(source);
  }, [open, source]);

  const chooseFile = async () => {
    const path = await pickPackFile(t("modpack.pickTitle"));
    if (path) void run({ path });
  };

  const percent = progress && "total" in progress && progress.total > 0 ? (progress.done / progress.total) * 100 : null;

  return (
    <>
      {dragging && (
        <div className="pointer-events-none fixed inset-3 z-[60] grid place-items-center rounded-xl border-2 border-dashed border-brand bg-background/80 text-foreground">
          <p className="flex items-center gap-2 font-display text-lg">
            <FileArchive className="size-5" />{" "}
            {dragging === "instance" && target ? t("drop.intoInstance", { name: target.name }) : t("modpack.dropHere")}
          </p>
        </div>
      )}
      <Dialog open={open} onOpenChange={(o) => !o && close()}>
        <DialogContent showCloseButton={!running} className="gap-5 border-surface-3 bg-surface-1 sm:max-w-[520px]">
          <DialogHeader>
            <DialogTitle className="font-display text-lg font-medium">
              {running && source?.name ? t("modpack.installing", { name: source.name }) : t("modpack.importTitle")}
            </DialogTitle>
            <DialogDescription>{t("modpack.importDescription")}</DialogDescription>
          </DialogHeader>

          {launchers && !running ? (
            <LauncherImport
              onBack={() => setLaunchers(false)}
              onDone={(instance) => {
                void refreshInstances();
                showToast(t("launchers.imported", { name: instance.name }));
                close();
                navigate(`/instances/${instance.id}`);
              }}
            />
          ) : running ? (
            <div className="flex flex-col gap-2 py-2">
              <p className="flex items-center gap-2 text-[13px]">
                <Loader2 className="size-4 animate-spin text-ink-muted" />
                {t(`modpack.stage.${progress?.stage ?? "start"}`)}
                {progress && "total" in progress && progress.total > 0 && (
                  <span className="ml-auto text-xs text-ink-muted tabular-nums">
                    {progress.done} / {progress.total}
                  </span>
                )}
              </p>
              <div className="h-1.5 overflow-hidden rounded-full bg-surface-3">
                <div
                  className={cn("h-full bg-brand transition-[width]", percent === null && "w-1/3 animate-pulse")}
                  style={percent === null ? undefined : { width: `${percent}%` }}
                />
              </div>
            </div>
          ) : (
            <div className="flex flex-col gap-3">
              <button
                type="button"
                onClick={() => void chooseFile()}
                className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-surface-3 bg-surface-0 px-6 py-7 text-center outline-none transition-colors hover:border-ink-muted focus-visible:ring-2 focus-visible:ring-ring"
              >
                <FileArchive className="size-7 text-ink-muted" />
                <span className="text-[13px] text-foreground">{t("modpack.chooseFile")}</span>
                <span className="text-xs text-ink-muted">{t("modpack.chooseFileHint")}</span>
              </button>
              <form
                className="flex gap-2"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (url.trim()) void run({ url: url.trim() });
                }}
              >
                <label className="relative flex-1">
                  <Link2 className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-muted" />
                  <Input
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                    placeholder={t("modpack.urlPlaceholder")}
                    aria-label={t("modpack.urlLabel")}
                    className="h-10 bg-surface-0 pl-9"
                  />
                </label>
                <Button type="submit" variant="secondary" className="h-10" disabled={!url.trim()}>
                  {t("modpack.importUrl")}
                </Button>
              </form>
              <button
                type="button"
                onClick={() => setLaunchers(true)}
                className="flex items-center gap-3 rounded-lg border border-surface-3 bg-surface-0 px-4 py-3 text-left outline-none transition-colors hover:border-ink-muted focus-visible:ring-2 focus-visible:ring-ring"
              >
                <Layers className="size-5 shrink-0 text-ink-muted" />
                <span>
                  <span className="block text-[13px] text-foreground">{t("launchers.title")}</span>
                  <span className="block text-xs text-ink-muted">{t("launchers.hint")}</span>
                </span>
              </button>
              {error && (
                <p role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-[13px] text-destructive">
                  {error}
                </p>
              )}
            </div>
          )}

          {!running && (
            <DialogFooter>
              <Button variant="ghost" onClick={close}>
                {t("modpack.cancel")}
              </Button>
            </DialogFooter>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}

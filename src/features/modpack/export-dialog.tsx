import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { File, Folder, Loader2 } from "lucide-react";
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
import { contentApi } from "@/features/content/api";
import type { Instance } from "@/features/instances/types";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { modpackApi, packFormats, pickSavePath, type ExportEntry, type PackFormat, type PackProgress } from "./api";

function formatSize(bytes: number, locale: string) {
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: unit ? 1 : 0 }).format(value)} ${units[unit]}`;
}

export function ExportDialog({
  instance,
  open,
  onOpenChange,
}: {
  instance: Instance;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t, i18n } = useTranslation();
  const [format, setFormat] = useState<PackFormat>("modrinth");
  const [name, setName] = useState(instance.name);
  const [version, setVersion] = useState("1.0.0");
  const [author, setAuthor] = useState("");
  const [entries, setEntries] = useState<ExportEntry[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [curseforge, setCurseforge] = useState(true);
  const [progress, setProgress] = useState<PackProgress | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setName(instance.name);
    setError(null);
    setEntries(null);
    modpackApi
      .entries(instance.id)
      .then((list) => {
        setEntries(list);
        setSelected(new Set(list.filter((e) => e.suggested).map((e) => e.name)));
      })
      .catch((e) => setError(errorText(e)));
    contentApi
      .sources()
      .then((s) => setCurseforge(s.curseforge))
      .catch(() => setCurseforge(false));
  }, [open, instance.id]);

  const toggle = (entry: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(entry)) next.delete(entry);
      else next.add(entry);
      return next;
    });

  const run = async () => {
    const fileName = name.trim().replace(/[\\/:*?"<>|]/g, "_") || "modpack";
    const path = await pickSavePath(t("modpack.saveTitle"), `${fileName}-${version.trim() || "1.0.0"}`, format);
    if (!path) return;
    setRunning(true);
    setError(null);
    try {
      const include = (entries ?? []).filter((e) => selected.has(e.name)).map((e) => e.name);
      const result = await modpackApi.export(
        instance.id,
        { format, path, name: name.trim() || instance.name, version: version.trim() || "1.0.0", author, summary: "", include },
        setProgress,
      );
      showToast(
        t("modpack.exported", {
          linked: result.linked,
          bundled: result.bundled,
          size: formatSize(result.size, i18n.language),
        }),
      );
      onOpenChange(false);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setRunning(false);
      setProgress(null);
    }
  };

  const percent = progress && "total" in progress && progress.total > 0 ? (progress.done / progress.total) * 100 : null;

  return (
    <Dialog open={open} onOpenChange={(o) => !running && onOpenChange(o)}>
      <DialogContent showCloseButton={!running} className="gap-5 border-surface-3 bg-surface-1 sm:max-w-[560px]">
        <DialogHeader>
          <DialogTitle className="font-display text-lg font-medium">{t("modpack.exportTitle")}</DialogTitle>
          <DialogDescription>{t("modpack.exportDescription")}</DialogDescription>
        </DialogHeader>

        <div role="tablist" aria-label={t("modpack.format")} className="grid grid-cols-2 gap-2">
          {packFormats.map((key) => (
            <button
              key={key}
              type="button"
              role="tab"
              aria-selected={format === key}
              disabled={running}
              onClick={() => setFormat(key)}
              className={cn(
                "flex flex-col items-start gap-0.5 rounded-lg border border-surface-3 bg-surface-0 px-3 py-2.5 text-left outline-none transition-colors hover:border-ink-muted focus-visible:ring-2 focus-visible:ring-ring",
                format === key && "border-brand hover:border-brand",
              )}
            >
              <span className="text-[13px] text-foreground">{t(`modpack.formats.${key}`)}</span>
              <span className="text-xs text-ink-muted">{t(`modpack.formatHint.${key}`)}</span>
            </button>
          ))}
        </div>
        {format === "curseforge" && !curseforge && (
          <p className="-mt-2 text-xs text-[#e0951a]">{t("modpack.noCurseforgeKey")}</p>
        )}

        <div className="grid grid-cols-[1fr_120px] gap-2">
          <label className="flex flex-col gap-1 text-xs text-ink-muted">
            {t("modpack.name")}
            <Input value={name} onChange={(e) => setName(e.target.value)} disabled={running} className="h-9 bg-surface-0" />
          </label>
          <label className="flex flex-col gap-1 text-xs text-ink-muted">
            {t("modpack.version")}
            <Input value={version} onChange={(e) => setVersion(e.target.value)} disabled={running} className="h-9 bg-surface-0" />
          </label>
          {format === "curseforge" && (
            <label className="col-span-2 flex flex-col gap-1 text-xs text-ink-muted">
              {t("modpack.author")}
              <Input value={author} onChange={(e) => setAuthor(e.target.value)} disabled={running} className="h-9 bg-surface-0" />
            </label>
          )}
        </div>

        <section className="flex flex-col gap-1.5">
          <h3 className="text-xs text-ink-muted">{t("modpack.include")}</h3>
          <ul className="max-h-56 overflow-y-auto rounded-lg border border-surface-3 bg-surface-0 p-1">
            {entries === null ? (
              <li className="flex h-20 items-center justify-center text-ink-muted">
                <Loader2 className="size-4 animate-spin" />
              </li>
            ) : entries.length === 0 ? (
              <li className="px-3 py-4 text-center text-[13px] text-ink-muted">{t("modpack.emptyDir")}</li>
            ) : (
              entries.map((entry) => (
                <li key={entry.name}>
                  <label className="flex h-8 cursor-pointer items-center gap-2.5 rounded px-2 text-[13px] hover:bg-surface-2">
                    <input
                      type="checkbox"
                      checked={selected.has(entry.name)}
                      disabled={running}
                      onChange={() => toggle(entry.name)}
                      className="size-3.5 accent-brand"
                    />
                    {entry.isDir ? (
                      <Folder className="size-3.5 text-ink-muted" />
                    ) : (
                      <File className="size-3.5 text-ink-muted" />
                    )}
                    <span className="min-w-0 flex-1 truncate">{entry.name}</span>
                    <span className="text-xs text-ink-muted tabular-nums">{formatSize(entry.size, i18n.language)}</span>
                  </label>
                </li>
              ))
            )}
          </ul>
        </section>

        {running && (
          <div className="flex flex-col gap-1.5">
            <p className="flex items-center gap-2 text-xs text-ink-muted">
              <Loader2 className="size-3.5 animate-spin" /> {t(`modpack.stage.${progress?.stage ?? "start"}`)}
            </p>
            <div className="h-1.5 overflow-hidden rounded-full bg-surface-3">
              <div
                className={cn("h-full bg-brand transition-[width]", percent === null && "w-1/3 animate-pulse")}
                style={percent === null ? undefined : { width: `${percent}%` }}
              />
            </div>
          </div>
        )}
        {error && (
          <p role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-[13px] text-destructive">
            {error}
          </p>
        )}

        <DialogFooter>
          <Button variant="ghost" disabled={running} onClick={() => onOpenChange(false)}>
            {t("modpack.cancel")}
          </Button>
          <Button disabled={running || !entries || selected.size === 0} onClick={() => void run()}>
            {running && <Loader2 className="animate-spin" />}
            {t("modpack.exportButton")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

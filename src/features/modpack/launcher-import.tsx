import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, FolderSearch, Loader2, Package } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ProgressBar, type ByteProgress } from "@/components/ui/progress-bar";
import { channel, isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import type { Instance } from "@/features/instances/types";

export interface FoundInstance {
  launcher: "prism" | "multimc" | "curseforge" | "modrinth" | "minecraft";
  name: string;
  path: string;
  mcVersion: string;
  loader: "fabric" | "quilt" | "forge" | "neoforge" | null;
  loaderVersion: string | null;
  mods: number;
}

const previewFound: FoundInstance[] = [
  { launcher: "prism", name: "Fabulously Optimized", path: "C:/Prism/instances/FO/minecraft", mcVersion: "1.21.1", loader: "fabric", loaderVersion: "0.16.5", mods: 84 },
  { launcher: "minecraft", name: "1.20.1-forge-47.2.0", path: "C:/.minecraft", mcVersion: "1.20.1", loader: "forge", loaderVersion: "47.2.0", mods: 12 },
];

const scan = (path?: string) => (isTauri ? invoke<FoundInstance[]>("launcher_scan", { path: path ?? null }) : Promise.resolve(previewFound));
const importOne = (found: FoundInstance, onProgress: (p: ByteProgress) => void) =>
  invoke<Instance>("launcher_import", { found, progress: channel(onProgress) });

const loaderName: Record<string, string> = { fabric: "Fabric", quilt: "Quilt", forge: "Forge", neoforge: "NeoForge" };

/** Сборки из других лаунчеров: найденные автоматически или в выбранной папке. */
export function LauncherImport({ onBack, onDone }: { onBack: () => void; onDone: (instance: Instance) => void }) {
  const { t } = useTranslation();
  const [found, setFound] = useState<FoundInstance[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [progress, setProgress] = useState<ByteProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = (path?: string) => {
    setFound(null);
    setError(null);
    scan(path).then(setFound, (e) => {
      setFound([]);
      setError(errorText(e));
    });
  };
  useEffect(() => load(), []);

  const pickFolder = async () => {
    if (!isTauri) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const dir = await open({ directory: true, title: t("launchers.pickFolder") });
    if (typeof dir === "string") load(dir);
  };

  const run = async (f: FoundInstance) => {
    setBusy(f.path + f.name);
    setProgress(null);
    setError(null);
    try {
      onDone(await importOne(f, setProgress));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <Button variant="ghost" size="sm" disabled={busy !== null} onClick={onBack} className="text-ink-muted">
          <ArrowLeft /> {t("launchers.back")}
        </Button>
        <span className="flex-1" />
        <Button variant="ghost" size="sm" disabled={busy !== null} onClick={() => void pickFolder()}>
          <FolderSearch /> {t("launchers.otherFolder")}
        </Button>
      </div>
      {found === null ? (
        <p className="flex items-center gap-2 py-4 text-[13px] text-ink-muted">
          <Loader2 className="size-4 animate-spin" /> {t("launchers.searching")}
        </p>
      ) : found.length === 0 ? (
        <p className="py-4 text-[13px] text-ink-muted">{t("launchers.nothing")}</p>
      ) : (
        <ul className="flex max-h-[50vh] flex-col divide-y divide-surface-3 overflow-y-auto rounded-lg border border-surface-3 bg-surface-0">
          {found.map((f) => {
            const key = f.path + f.name;
            return (
              <li key={key} className="flex items-center gap-3 px-3 py-2.5">
                <Package className="size-5 shrink-0 text-ink-muted" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13px] text-foreground">{f.name}</p>
                  {busy === key ? (
                    <ProgressBar progress={progress} className="mt-1" />
                  ) : (
                    <p className="truncate text-xs text-ink-muted">
                      {[t(`launchers.from.${f.launcher}`), `${f.mcVersion}${f.loader ? ` · ${loaderName[f.loader]}` : ""}`, f.mods ? t("launchers.mods", { count: f.mods }) : null]
                        .filter(Boolean)
                        .join(" · ")}
                    </p>
                  )}
                </div>
                <Button size="sm" variant="secondary" disabled={busy !== null} onClick={() => void run(f)} className={cn("shrink-0")}>
                  {busy === key ? <Loader2 className="animate-spin" /> : null}
                  {t("launchers.import")}
                </Button>
              </li>
            );
          })}
        </ul>
      )}
      {error && (
        <p role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-[13px] text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}

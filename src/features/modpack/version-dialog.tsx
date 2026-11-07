import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Download, History, Loader2, RotateCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { showToast } from "@/components/toast";
import { refreshInstances } from "@/features/instances/store";
import type { Instance } from "@/features/instances/types";
import { openManual, refreshContent } from "@/features/content/store";
import { errorText } from "@/lib/errors";
import { createStore } from "@/lib/store";
import { timeAgo } from "@/lib/time";
import { cn } from "@/lib/utils";
import { ProgressBar } from "@/components/ui/progress-bar";
import { modpackApi, type PackProgress, type PackVersion } from "./api";
import { openImport } from "./import-dialog";

type Target =
  | { mode: "install"; source: "modrinth" | "curseforge"; projectId: string; name: string; url?: string }
  | { mode: "change"; instance: Instance };

const store = createStore<Target | null>(null);

/** Выбор версии модпака перед установкой. */
export const openPackVersions = (target: Extract<Target, { mode: "install" }>) => store.set(target);
/** Обновление или откат модпака уже поставленной сборки. */
export const openChangeVersion = (instance: Instance) => store.set({ mode: "change", instance });

const ALL = "";

export function PackVersionDialog() {
  const { t, i18n } = useTranslation();
  const target = store.use();
  const [versions, setVersions] = useState<PackVersion[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [game, setGame] = useState(ALL);
  const [loader, setLoader] = useState(ALL);
  const [betas, setBetas] = useState(false);
  const [progress, setProgress] = useState<PackProgress | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const source = target?.mode === "change" ? target.instance.pack?.source : target?.source;
  const projectId = target?.mode === "change" ? target.instance.pack?.projectId : target?.projectId;
  const current = target?.mode === "change" ? target.instance.pack?.versionId : undefined;

  const load = () => {
    if (!source || !projectId) return;
    setVersions(null);
    setError(null);
    modpackApi.versions(source, projectId).then(setVersions, (e) => setError(errorText(e)));
  };

  useEffect(() => {
    setGame(ALL);
    setLoader(ALL);
    setBetas(false);
    if (target) load();
  }, [target]);

  const close = () => {
    if (busy) return;
    store.set(null);
  };

  const games = useMemo(() => [...new Set((versions ?? []).flatMap((v) => v.gameVersions))].sort(compareVersions), [versions]);
  const loaders = useMemo(() => [...new Set((versions ?? []).flatMap((v) => v.loaders))].sort(), [versions]);
  const shown = (versions ?? []).filter(
    (v) =>
      (betas || v.kind === "release" || v.id === current) &&
      (game === ALL || v.gameVersions.includes(game)) &&
      (loader === ALL || v.loaders.includes(loader)),
  );
  // Если релизов нет совсем, показываем беты сразу, иначе список пустой
  const onlyBetas = (versions ?? []).length > 0 && !(versions ?? []).some((v) => v.kind === "release");
  const list = onlyBetas && !betas ? (versions ?? []).filter((v) => (game === ALL || v.gameVersions.includes(game)) && (loader === ALL || v.loaders.includes(loader))) : shown;
  const currentIndex = (versions ?? []).findIndex((v) => v.id === current);

  const choose = async (v: PackVersion) => {
    if (!target) return;
    if (target.mode === "install") {
      store.set(null);
      openImport(
        target.source === "curseforge"
          ? { curseforgeId: Number(target.projectId), versionId: v.id, name: target.name }
          : { url: target.url ?? `https://modrinth.com/modpack/${target.projectId}`, versionId: v.id, name: target.name },
      );
      return;
    }
    setBusy(v.id);
    setProgress(null);
    try {
      const result = await modpackApi.changeVersion(target.instance.id, v.id, setProgress);
      await Promise.all([refreshInstances(), refreshContent(target.instance.id)]);
      showToast(t("packVersion.changed", { name: target.instance.name, version: v.name }));
      if (result.manual.length) openManual(target.instance.id, result.manual);
      store.set(null);
    } catch (e) {
      showToast(t("packVersion.failed", { error: errorText(e) }));
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };

  const name = target?.mode === "change" ? target.instance.name : target?.name;
  const select =
    "h-8 appearance-none rounded-md border border-surface-3 bg-surface-0 pr-8 pl-2.5 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring";

  return (
    <Dialog open={!!target} onOpenChange={(o) => !o && close()}>
      <DialogContent showCloseButton={!busy} className="flex max-h-[85vh] flex-col gap-4 border-surface-3 bg-surface-1 sm:max-w-[620px]">
        <DialogHeader>
          <DialogTitle className="font-display text-lg font-medium">
            {target?.mode === "change" ? t("packVersion.changeTitle", { name }) : t("packVersion.installTitle", { name })}
          </DialogTitle>
          <DialogDescription>
            {target?.mode === "change"
              ? t("packVersion.changeText", { version: target.instance.pack?.versionName ?? "?" })
              : t("packVersion.installText")}
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-wrap items-center gap-2">
          <span className="relative">
            <select value={game} onChange={(e) => setGame(e.target.value)} aria-label={t("packVersion.game")} className={select}>
              <option value={ALL}>{t("packVersion.anyGame")}</option>
              {games.map((g) => (
                <option key={g} value={g}>
                  Minecraft {g}
                </option>
              ))}
            </select>
            <ChevronDown className="pointer-events-none absolute top-1/2 right-2.5 size-3.5 -translate-y-1/2 text-ink-muted" />
          </span>
          {loaders.length > 1 && (
            <span className="relative">
              <select value={loader} onChange={(e) => setLoader(e.target.value)} aria-label={t("packVersion.loader")} className={select}>
                <option value={ALL}>{t("packVersion.anyLoader")}</option>
                {loaders.map((l) => (
                  <option key={l} value={l}>
                    {t(`loader.${l}`, { defaultValue: l })}
                  </option>
                ))}
              </select>
              <ChevronDown className="pointer-events-none absolute top-1/2 right-2.5 size-3.5 -translate-y-1/2 text-ink-muted" />
            </span>
          )}
          <label className="ml-auto flex cursor-pointer items-center gap-2 text-xs text-ink-muted select-none hover:text-foreground">
            <input type="checkbox" checked={betas || onlyBetas} disabled={onlyBetas} onChange={(e) => setBetas(e.target.checked)} className="size-3.5 accent-brand" />
            {t("packVersion.betas")}
          </label>
        </div>

        <div className="min-h-[200px] flex-1 overflow-y-auto rounded-lg border border-surface-3">
          {error ? (
            <div className="flex flex-col items-center gap-3 p-8 text-center text-[13px] text-ink-muted">
              {error}
              <Button variant="secondary" size="sm" onClick={load}>
                <RotateCw /> {t("create.retry")}
              </Button>
            </div>
          ) : versions === null ? (
            <p className="flex items-center gap-2 p-6 text-[13px] text-ink-muted">
              <Loader2 className="size-4 animate-spin" /> {t("packVersion.loading")}
            </p>
          ) : list.length === 0 ? (
            <p className="p-6 text-[13px] text-ink-muted">{t("packVersion.none")}</p>
          ) : (
            <ul className="divide-y divide-surface-3">
              {list.map((v) => {
                const isCurrent = v.id === current;
                const index = (versions ?? []).indexOf(v);
                // Список новые сверху: ниже текущей — откат, выше — обновление
                const older = currentIndex >= 0 && index > currentIndex;
                const running = busy === v.id;
                return (
                  <li key={v.id} className={cn("flex items-center gap-3 px-4 py-2.5", isCurrent && "bg-surface-2/60")}>
                    <div className="min-w-0 flex-1">
                      <p className="flex items-center gap-2 truncate text-[13px] text-foreground">
                        <span className="truncate">{v.name}</span>
                        {v.kind !== "release" && (
                          <span className="rounded bg-[#e0951a]/15 px-1.5 py-px text-[10px] font-medium text-[#e0951a] uppercase">
                            {t(`packVersion.kind.${v.kind}`)}
                          </span>
                        )}
                        {isCurrent && (
                          <span className="rounded bg-brand/15 px-1.5 py-px text-[10px] font-medium text-brand uppercase">
                            {t("packVersion.current")}
                          </span>
                        )}
                      </p>
                      <p className="truncate text-xs text-ink-muted">
                        {[
                          v.gameVersions.length ? `Minecraft ${v.gameVersions.slice(0, 3).join(", ")}` : null,
                          v.loaders.map((l) => t(`loader.${l}`, { defaultValue: l })).join(", ") || null,
                          v.date ? timeAgo(v.date, i18n.language) : null,
                        ]
                          .filter(Boolean)
                          .join(" · ")}
                      </p>
                      {running && (
                        <ProgressBar
                          className="mt-2"
                          unit="count"
                          label={t(`modpack.stage.${progress?.stage ?? "start"}`)}
                          progress={progress && "total" in progress ? { done: progress.done, total: progress.total } : null}
                        />
                      )}
                    </div>
                    <Button
                      size="sm"
                      variant={isCurrent ? "ghost" : "secondary"}
                      disabled={isCurrent || !!busy || v.manual}
                      title={v.manual ? t("packVersion.manual") : undefined}
                      onClick={() => void choose(v)}
                      className="w-[128px] shrink-0"
                    >
                      {running ? (
                        <Loader2 className="animate-spin" />
                      ) : target?.mode === "change" && older ? (
                        <History />
                      ) : (
                        <Download />
                      )}
                      {isCurrent
                        ? t("packVersion.installed")
                        : target?.mode === "change"
                          ? older
                            ? t("packVersion.rollback")
                            : t("packVersion.update")
                          : t("content.install")}
                    </Button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
        {target?.mode === "change" && <p className="text-xs text-ink-muted">{t("packVersion.keepsWorlds")}</p>}
      </DialogContent>
    </Dialog>
  );
}

/** Новые версии игры сверху: 1.21.1 > 1.21 > 1.20.6. */
function compareVersions(a: string, b: string) {
  const pa = a.split(/[.-]/).map((n) => parseInt(n, 10) || 0);
  const pb = b.split(/[.-]/).map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pb[i] ?? 0) - (pa[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

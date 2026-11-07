import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Loader2, RotateCw, Search } from "lucide-react";
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
import { cn } from "@/lib/utils";
import { fetchVersions, type VersionManifest } from "@/features/versions/api";
import { createInstance } from "./store";
import { InstanceIcon } from "./instance-icon";
import { LoaderVersionSelect } from "./loader-version-select";
import { VersionTypeBadge } from "./instance-meta";
import { loadersFor, modLoaders, type ModLoader, type VersionType } from "./types";

type Filter = "release" | "snapshot" | "old";

const filterTypes: Record<Filter, VersionType[]> = {
  release: ["release"],
  snapshot: ["snapshot"],
  old: ["old_beta", "old_alpha"],
};

export function CreateInstanceDialog({
  open,
  onOpenChange,
  initialVersion,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Версия, выбранная при открытии (например, из новостей) */
  initialVersion?: string | null;
}) {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const [manifest, setManifest] = useState<VersionManifest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("release");
  const [query, setQuery] = useState("");
  const [versionId, setVersionId] = useState<string | null>(null);
  const [loader, setLoader] = useState<ModLoader | null>(null);
  const [loaderVersion, setLoaderVersion] = useState<string | null>(null);
  const [name, setName] = useState("");

  const load = () => {
    setError(null);
    fetchVersions()
      .then((m) => {
        setManifest(m);
        setVersionId((current) => current ?? m.latest.release);
      })
      .catch((e) => setError(String(e)));
  };

  useEffect(() => {
    if (open && !manifest) load();
  }, [open]);

  // Открыли с конкретной версией — выбираем её и нужный фильтр
  useEffect(() => {
    if (!open || !initialVersion || !manifest) return;
    const v = manifest.versions.find((x) => x.id === initialVersion);
    if (!v) return;
    const next = (Object.keys(filterTypes) as Filter[]).find((f) => filterTypes[f].includes(v.type));
    if (next) setFilter(next);
    setQuery("");
    setVersionId(v.id);
  }, [open, initialVersion, manifest]);

  const versions = useMemo(() => {
    if (!manifest) return [];
    const q = query.trim().toLowerCase();
    return manifest.versions.filter(
      (v) => filterTypes[filter].includes(v.type) && (!q || v.id.toLowerCase().includes(q)),
    );
  }, [manifest, filter, query]);

  const selected = manifest?.versions.find((v) => v.id === versionId) ?? null;
  const allowedLoaders = selected ? loadersFor(selected.type) : [];

  // Если для новой версии выбранный загрузчик недоступен — возвращаемся к чистой ванилле
  useEffect(() => {
    if (loader && !allowedLoaders.includes(loader)) setLoader(null);
  }, [versionId]);

  // Версии загрузчика свои для каждой версии Minecraft — при смене сбрасываем на рекомендованную
  useEffect(() => setLoaderVersion(null), [versionId, loader]);

  const changeFilter = (next: Filter) => {
    setFilter(next);
    if (!manifest) return;
    const first =
      next === "release"
        ? manifest.latest.release
        : next === "snapshot"
          ? manifest.latest.snapshot
          : manifest.versions.find((v) => filterTypes.old.includes(v.type))?.id;
    if (first) setVersionId(first);
  };

  const defaultName = selected
    ? `${loader ? t(`loader.${loader}`) : t("core.vanilla")} ${selected.id}`
    : "";

  const [saving, setSaving] = useState(false);
  const create = async () => {
    if (!selected) return;
    setSaving(true);
    try {
      const instance = await createInstance({
        name: name.trim() || defaultName,
        mcVersion: selected.id,
        versionType: selected.type,
        loader,
        loaderVersion: loader ? loaderVersion : null,
      });
      onOpenChange(false);
      setName("");
      setLoader(null);
      navigate(`/instances/${instance.id}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const dateFmt = new Intl.DateTimeFormat(i18n.language, { day: "numeric", month: "short", year: "numeric" });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-5 border-surface-3 bg-surface-1 sm:max-w-[620px]">
        <DialogHeader>
          <DialogTitle className="font-display text-lg font-medium">{t("create.title")}</DialogTitle>
          <DialogDescription>{t("create.description")}</DialogDescription>
        </DialogHeader>

        <div className="flex items-center gap-3">
          <InstanceIcon name={name.trim() || defaultName || "new"} loader={loader} className="size-11" />
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={defaultName || t("create.namePlaceholder")}
            aria-label={t("create.name")}
            className="h-11 bg-surface-0"
          />
        </div>

        {/* Ядро — всегда ванильный Minecraft; загрузчик модов выбирается отдельно ниже */}
        <section className="flex flex-col gap-2">
          <div className="flex items-end justify-between gap-3">
            <div>
              <h3 className="text-[13px] text-foreground">{t("create.core")}</h3>
              <p className="text-xs text-ink-muted">{t("create.coreHint")}</p>
            </div>
            <div role="tablist" className="flex rounded-md bg-surface-2 p-0.5">
              {(Object.keys(filterTypes) as Filter[]).map((key) => (
                <button
                  key={key}
                  type="button"
                  role="tab"
                  aria-selected={filter === key}
                  onClick={() => changeFilter(key)}
                  className={cn(
                    "h-7 rounded px-2.5 text-xs text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                    filter === key && "bg-surface-0 text-foreground",
                  )}
                >
                  {t(`create.filter.${key}`)}
                </button>
              ))}
            </div>
          </div>

          <div className="overflow-hidden rounded-lg border border-surface-3 bg-surface-0">
            <label className="flex h-9 items-center gap-2 border-b border-surface-3 px-3">
              <Search className="size-3.5 text-ink-muted" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("create.searchVersion")}
                className="h-full flex-1 bg-transparent text-[13px] outline-none placeholder:text-ink-muted"
              />
            </label>
            <div role="listbox" aria-label={t("create.core")} className="h-52 overflow-y-auto p-1">
              {error ? (
                <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-[13px] text-ink-muted">
                  <p>{t("create.loadError")}</p>
                  <Button size="sm" variant="secondary" onClick={load}>
                    <RotateCw /> {t("create.retry")}
                  </Button>
                </div>
              ) : !manifest ? (
                <div className="flex h-full items-center justify-center text-ink-muted">
                  <Loader2 className="size-5 animate-spin" aria-label={t("create.loading")} />
                </div>
              ) : versions.length === 0 ? (
                <p className="flex h-full items-center justify-center text-[13px] text-ink-muted">
                  {t("create.noVersions")}
                </p>
              ) : (
                versions.map((v) => {
                  const isLatest = v.id === manifest.latest.release || v.id === manifest.latest.snapshot;
                  return (
                    <button
                      key={v.id}
                      type="button"
                      role="option"
                      aria-selected={v.id === versionId}
                      onClick={() => setVersionId(v.id)}
                      className={cn(
                        "flex h-8 w-full items-center gap-2 rounded-md px-2.5 text-left text-[13px] outline-none transition-colors hover:bg-surface-2 focus-visible:bg-surface-2",
                        v.id === versionId && "bg-surface-3 hover:bg-surface-3",
                      )}
                    >
                      <span className="font-medium text-foreground">{v.id}</span>
                      {filter === "old" && <VersionTypeBadge type={v.type} />}
                      {isLatest && <span className="text-xs text-ink-muted">{t("create.latest")}</span>}
                      <span className="ml-auto text-xs text-ink-muted tabular-nums">
                        {dateFmt.format(new Date(v.releaseTime))}
                      </span>
                    </button>
                  );
                })
              )}
            </div>
          </div>
        </section>

        <section className="flex flex-col gap-2">
          <div>
            <h3 className="text-[13px] text-foreground">{t("create.loader")}</h3>
            <p className="text-xs text-ink-muted">
              {selected?.type === "snapshot"
                ? t("create.loaderSnapshotHint")
                : selected && allowedLoaders.length === 0
                  ? t("create.loaderOldHint")
                  : t("create.loaderHint")}
            </p>
          </div>
          <div role="radiogroup" aria-label={t("create.loader")} className="grid grid-cols-5 gap-1.5">
            {[null, ...modLoaders].map((key) => {
              const disabled = key !== null && !allowedLoaders.includes(key);
              const active = loader === key;
              return (
                <button
                  key={key ?? "none"}
                  type="button"
                  role="radio"
                  aria-checked={active}
                  disabled={disabled}
                  onClick={() => setLoader(key)}
                  className={cn(
                    "flex h-14 flex-col items-center justify-center gap-1 rounded-md border border-surface-3 bg-surface-0 text-[13px] outline-none transition-colors hover:border-ink-muted/60 focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-35",
                    active && "border-foreground hover:border-foreground",
                  )}
                >
                  <InstanceIcon name={key ?? "vanilla"} loader={key} className="size-4 [--step:1px]" />
                  {key ? t(`loader.${key}`) : t("create.noLoader")}
                </button>
              );
            })}
          </div>
          {loader && selected && (
            <div className="flex items-center gap-3 pt-1">
              <label htmlFor="create-loader-version" className="text-xs text-ink-muted">
                {t("loaderVersion.label", { loader: t(`loader.${loader}`) })}
              </label>
              <LoaderVersionSelect
                id="create-loader-version"
                loader={loader}
                mcVersion={selected.id}
                value={loaderVersion}
                onChange={setLoaderVersion}
              />
            </div>
          )}
        </section>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("create.cancel")}
          </Button>
          <Button onClick={() => void create()} disabled={!selected || saving}>
            {t("create.submit")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

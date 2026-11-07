import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowUpCircle, ChevronLeft, ChevronRight, Download, Link2, Loader2, Package, Plus, RefreshCw, Search, Trash2, Zap } from "lucide-react";
import { Button } from "@/components/ui/button";
import { isTauri } from "@/lib/backend";
import { EmptyState } from "@/components/page";
import { ProjectIcon } from "@/components/project-icon";
import { cn } from "@/lib/utils";
import type { Instance } from "@/features/instances/types";
import { contentKinds, type ContentKind, type InstalledContent, type UpdateInfo } from "./api";
import { ContentBrowser } from "./content-browser";
import { ProgressBar } from "@/components/ui/progress-bar";
import { showToast } from "@/components/toast";
import { errorText } from "@/lib/errors";
import { installDeps, missingDeps, type MissingDep } from "./store";
import { applyUpdates, checkUpdates, refreshContent, removeContent, toggleContent, useContent } from "./store";

const key = (c: { kind: ContentKind; file: string }) => `${c.kind}/${c.file}`;

/** Моды, ресурспаки и шейдеры сборки: включение, обновление, удаление. */
export function InstanceContent({ instance, locked }: { instance: Instance; locked: boolean }) {
  const { t } = useTranslation();
  const state = useContent(instance.id);
  const [kind, setKind] = useState<ContentKind>(instance.loader ? "mod" : "resourcepack");
  const [filter, setFilter] = useState("");
  const [adding, setAdding] = useState(false);

  useEffect(() => {
    void refreshContent(instance.id);
  }, [instance.id]);

  const items = state.items ?? [];
  const counts = useMemo(
    () => Object.fromEntries(contentKinds.map((k) => [k, items.filter((c) => c.kind === k).length])) as Record<ContentKind, number>,
    [items],
  );
  const updates = useMemo(() => new Map((state.updates ?? []).map((u) => [key(u), u])), [state.updates]);
  const q = filter.trim().toLowerCase();
  const shown = items.filter(
    (c) => c.kind === kind && (!q || c.file.toLowerCase().includes(q) || c.meta?.title.toLowerCase().includes(q)),
  );
  const kindUpdates = (state.updates ?? []).filter((u) => u.kind === kind);
  // Длинный список — страницами: сотни строк с иконками сразу заметно тормозят
  const [page, setPage] = useState(0);
  useEffect(() => setPage(0), [kind, q]);
  const pages = Math.max(1, Math.ceil(shown.length / PAGE));
  const current = Math.min(page, pages - 1);
  const visible = shown.slice(current * PAGE, (current + 1) * PAGE);

  const add = () => setAdding(true);
  // Недостающие зависимости: null — не проверяли
  const [deps, setDeps] = useState<MissingDep[] | null>(null);
  const [depsBusy, setDepsBusy] = useState(false);
  useEffect(() => setDeps(null), [instance.id]);

  const checkDeps = async () => {
    setDepsBusy(true);
    try {
      const list = await missingDeps(instance.id);
      setDeps(list);
      if (!list.length) showToast(t("deps.allGood"));
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setDepsBusy(false);
    }
  };

  const installAll = async () => {
    if (!deps) return;
    setDepsBusy(true);
    try {
      const result = await installDeps(instance.id, deps.map((d) => d.id));
      if (result.failed.length) showToast(t("deps.failed", { ids: result.failed.join(", ") }));
      else showToast(t("deps.installed", { count: result.done.length }));
      setDeps(deps.filter((d) => result.failed.includes(d.id)));
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setDepsBusy(false);
    }
  };

  const [optimizing, setOptimizing] = useState(false);
  const optimize = async () => {
    setOptimizing(true);
    try {
      const r = isTauri
        ? await invoke<{ installed: { title: string }[]; present: number; unavailable: number }>("content_optimize", { id: instance.id })
        : { installed: [], present: 0, unavailable: 0 };
      await refreshContent(instance.id);
      showToast(
        r.installed.length
          ? t("optimize.done", { names: r.installed.map((i) => i.title).join(", ") })
          : r.present
            ? t("optimize.already")
            : t("optimize.none"),
      );
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setOptimizing(false);
    }
  };

  if (adding) {
    return <ContentBrowser instance={instance} kind={kind} onKindChange={setKind} onBack={() => setAdding(false)} />;
  }

  return (
    <div className="flex flex-col gap-3 pb-6">
      <div className="flex flex-wrap items-center gap-2">
        <div role="tablist" className="flex gap-1">
          {contentKinds.map((k) => (
            <button
              key={k}
              role="tab"
              type="button"
              aria-selected={kind === k}
              onClick={() => setKind(k)}
              className={cn(
                "flex h-8 items-center gap-1.5 rounded-md px-3 text-[13px] text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                kind === k && "bg-surface-2 text-foreground",
              )}
            >
              {t(`content.kinds.${k}`)}
              <span className="text-xs text-ink-muted tabular-nums">{counts[k]}</span>
            </button>
          ))}
        </div>
        <div className="ml-auto flex items-center gap-2">
          {kindUpdates.length > 0 ? (
            <Button
              size="sm"
              variant="secondary"
              disabled={locked}
              onClick={() => void applyUpdates(instance.id, kindUpdates)}
            >
              <ArrowUpCircle /> {t("content.updateAll", { count: kindUpdates.length })}
            </Button>
          ) : (
            <Button
              size="sm"
              variant="ghost"
              disabled={state.checking || items.length === 0}
              onClick={() => void checkUpdates(instance.id)}
            >
              {state.checking ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              {t("content.checkUpdates")}
            </Button>
          )}
          {kind === "mod" && instance.loader && (
            <Button size="sm" variant="ghost" disabled={depsBusy || items.length === 0} onClick={() => void checkDeps()}>
              {depsBusy && deps === null ? <Loader2 className="animate-spin" /> : <Link2 />}
              {t("deps.check")}
            </Button>
          )}
          {kind === "mod" && instance.loader && (
            <Button size="sm" variant="ghost" disabled={optimizing || locked} onClick={() => void optimize()} title={t("optimize.hint")}>
              {optimizing ? <Loader2 className="animate-spin" /> : <Zap />}
              {t("optimize.button")}
            </Button>
          )}
          <Button size="sm" onClick={add}>
            <Plus /> {t("content.add")}
          </Button>
        </div>
      </div>

      {locked && <p className="text-xs text-ink-muted">{t("content.lockedWhileRunning")}</p>}
      {kind === "mod" && deps && deps.length > 0 && (
        <div className="flex flex-wrap items-start gap-3 rounded-lg border border-[#e0951a]/30 bg-[#e0951a]/8 px-4 py-3 text-[13px]">
          <div className="min-w-0 flex-1">
            <p className="text-[#e0951a]">{t("deps.missing", { count: deps.length })}</p>
            <ul className="mt-1 flex flex-col gap-0.5 text-xs text-ink-muted">
              {deps.map((d) => (
                <li key={d.id}>
                  <span className="font-mono text-ink">{d.id}</span> — {t("deps.neededBy", { mods: d.forMods.join(", ") })}
                </li>
              ))}
            </ul>
          </div>
          <Button size="sm" disabled={depsBusy || locked} onClick={() => void installAll()}>
            {depsBusy ? <Loader2 className="animate-spin" /> : <Download />} {t("deps.installAll")}
          </Button>
        </div>
      )}
      {state.updating && (
        <ProgressBar
          unit={state.updating.stage === "snapshot" ? "bytes" : "count"}
          progress={state.updating.total > 0 ? state.updating : null}
          label={state.updating.stage === "snapshot" ? t("snapshots.progress.beforeUpdate") : t("content.updatingAll")}
        />
      )}

      {state.items === null ? (
        <p className="flex items-center gap-2 py-6 text-[13px] text-ink-muted">
          <Loader2 className="size-4 animate-spin" /> {t("content.scanning")}
        </p>
      ) : counts[kind] === 0 ? (
        <EmptyState
          icon={<Package className="size-7" />}
          title={t(`content.empty.${kind}`)}
          text={kind !== "resourcepack" && !instance.loader ? t("content.needsLoaderText") : t("content.emptyText")}
          action={
            <Button onClick={add} disabled={kind !== "resourcepack" && !instance.loader}>
              <Search /> {t("content.findOnModrinth")}
            </Button>
          }
        />
      ) : (
        <>
          {counts[kind] > 8 && (
            <label className="flex h-9 items-center gap-2 rounded-md border border-surface-3 bg-surface-1 px-3">
              <Search className="size-3.5 text-ink-muted" />
              <input
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder={t("content.filterByName")}
                className="h-full flex-1 bg-transparent text-[13px] outline-none placeholder:text-ink-muted"
              />
            </label>
          )}
          <ul className="flex flex-col divide-y divide-surface-3 rounded-lg border border-surface-3 bg-surface-1">
            {visible.map((item) => (
              <ContentRow
                key={key(item)}
                item={item}
                update={updates.get(key(item))}
                busy={state.busy.includes(item.file)}
                locked={locked}
                instanceId={instance.id}
              />
            ))}
          </ul>
          {pages > 1 && <Pager page={current} pages={pages} onChange={setPage} />}
        </>
      )}
    </div>
  );
}

const PAGE = 40;

function Pager({ page, pages, onChange }: { page: number; pages: number; onChange: (page: number) => void }) {
  const { t } = useTranslation();
  // Первая, последняя и соседние с текущей; между ними — «…»
  const shown = [...new Set([0, page - 1, page, page + 1, pages - 1])].filter((p) => p >= 0 && p < pages).sort((a, b) => a - b);
  return (
    <nav aria-label={t("content.pages")} className="flex items-center justify-center gap-1">
      <Button variant="ghost" size="icon" disabled={page === 0} onClick={() => onChange(page - 1)} aria-label={t("content.prevPage")}>
        <ChevronLeft />
      </Button>
      {shown.map((p, i) => (
        <span key={p} className="flex items-center gap-1">
          {i > 0 && p - shown[i - 1] > 1 && <span className="px-1 text-ink-muted">…</span>}
          <Button
            variant={p === page ? "secondary" : "ghost"}
            size="sm"
            aria-current={p === page ? "page" : undefined}
            onClick={() => onChange(p)}
            className="min-w-8 tabular-nums"
          >
            {p + 1}
          </Button>
        </span>
      ))}
      <Button variant="ghost" size="icon" disabled={page === pages - 1} onClick={() => onChange(page + 1)} aria-label={t("content.nextPage")}>
        <ChevronRight />
      </Button>
    </nav>
  );
}

function ContentRow({
  item,
  update,
  busy,
  locked,
  instanceId,
}: {
  item: InstalledContent;
  update: UpdateInfo | undefined;
  busy: boolean;
  locked: boolean;
  instanceId: string;
}) {
  const { t } = useTranslation();
  const name = item.file.replace(/\.disabled$/, "");
  const title = item.meta?.title ?? name.replace(/\.(jar|zip)$/i, "");

  return (
    <li className={cn("flex items-center gap-3 px-4 py-2.5", !item.enabled && "opacity-55")}>
      <ProjectIcon url={item.meta?.iconUrl} className="size-9" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-[13px] text-foreground">{title}</p>
        <p className="truncate text-xs text-ink-muted">
          {item.meta ? `${item.meta.versionNumber} · ${name}` : `${name} · ${t("content.unknownSource")}`}
        </p>
      </div>
      {update && (
        <Button
          size="sm"
          variant="secondary"
          disabled={busy || locked}
          onClick={() => void applyUpdates(instanceId, [update])}
          title={t("content.updateTo", { version: update.versionNumber })}
          className="max-w-[200px]"
        >
          <ArrowUpCircle /> <span className="truncate">{update.versionNumber}</span>
        </Button>
      )}
      {busy && <Loader2 className="size-4 animate-spin text-ink-muted" />}
      <button
        type="button"
        role="switch"
        aria-checked={item.enabled}
        aria-label={t(item.enabled ? "content.disable" : "content.enable", { title })}
        disabled={busy || locked}
        onClick={() => void toggleContent(instanceId, item)}
        className={cn(
          "relative h-5 w-9 shrink-0 rounded-full border outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
          item.enabled ? "border-foreground bg-foreground" : "border-surface-3 bg-surface-3",
        )}
      >
        <span
          className={cn(
            "absolute top-1/2 size-3.5 -translate-y-1/2 rounded-full transition-all",
            item.enabled ? "left-[18px] bg-background" : "left-[2px] bg-ink-muted",
          )}
        />
      </button>
      <Button
        variant="ghost"
        size="icon"
        disabled={busy || locked}
        aria-label={t("content.remove", { title })}
        title={t("content.remove", { title })}
        onClick={() => void removeContent(instanceId, item)}
        className="size-8 hover:text-destructive"
      >
        <Trash2 />
      </Button>
    </li>
  );
}

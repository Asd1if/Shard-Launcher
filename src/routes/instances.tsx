import { useEffect, useMemo, useRef, useState } from "react";
import { Link, Navigate, useParams, useSearchParams } from "react-router";
import { useTranslation } from "react-i18next";
import {
  SlidersHorizontal,
  Camera,
  Boxes,
  ChevronDown,
  ChevronRight,
  Download,
  EllipsisVertical,
  Loader2,
  RefreshCw,
  FolderOpen,
  History,
  Copy,
  PackageOpen,
  Plus,
  Search,
  Server,
  Star,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { EmptyState, Page } from "@/components/page";
import { PlayButton } from "@/components/play-button";
import { showToast } from "@/components/toast";
import { InstanceIcon } from "@/features/instances/instance-icon";
import { useInstanceDetails, useInstanceLabel, VersionTypeBadge } from "@/features/instances/instance-meta";
import { byLastPlayed, setInstanceMeta, useInstances } from "@/features/instances/store";
import { CreateInstanceDialog } from "@/features/instances/create-instance-dialog";
import { InstanceSettingsForm } from "@/features/instances/instance-settings";
import { InstanceContextMenu } from "@/features/instances/instance-menu";
import { openInstanceDialog } from "@/features/instances/instance-dialogs";
import { WorldsTab, ScreenshotsTab } from "@/features/instances/worlds-tab";
import { PlaytimeButton, formatPlaytime, usePlaytimes } from "@/features/instances/playtime";
import type { Instance } from "@/features/instances/types";
import { InstanceContent } from "@/features/content/instance-content";
import { openImport, setDropTarget } from "@/features/modpack/import-dialog";
import { openChangeVersion } from "@/features/modpack/version-dialog";
import { launch, useLaunch } from "@/features/launch/store";
import { backend } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { ShareLogButton } from "@/features/logs/share-log";
import { timeAgo } from "@/lib/time";
import { PackUpdateButton, usePackUpdate } from "@/features/modpack/pack-update";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

type Sort = "played" | "name" | "created" | "version";
const sorts: Sort[] = ["played", "name", "created", "version"];

function readPref<T extends string>(key: string, fallback: T, allowed?: readonly T[]): T {
  try {
    const v = localStorage.getItem(key) as T | null;
    return v && (!allowed || allowed.includes(v)) ? v : fallback;
  } catch {
    return fallback;
  }
}

function writePref(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // без хранилища просто не запоминаем
  }
}

const versionKey = (v: string) => v.split(/[.-]/).map((n) => (parseInt(n, 10) || 0).toString().padStart(4, "0")).join(".");

const comparators: Record<Sort, (a: Instance, b: Instance) => number> = {
  played: byLastPlayed,
  name: (a, b) => a.name.localeCompare(b.name),
  created: (a, b) => b.createdAt.localeCompare(a.createdAt),
  version: (a, b) => versionKey(b.mcVersion).localeCompare(versionKey(a.mcVersion)) || a.name.localeCompare(b.name),
};

export function InstancesPage() {
  const { t } = useTranslation();
  const instances = useInstances();
  const [creating, setCreating] = useState(false);
  const [query, setQuery] = useState("");
  const [sort, setSortState] = useState<Sort>(() => readPref("shard.instanceSort", "played", sorts));
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set(JSON.parse(readPref("shard.collapsedGroups", "[]"))));

  const setSort = (s: Sort) => {
    writePref("shard.instanceSort", s);
    setSortState(s);
  };
  const toggleGroup = (key: string) => {
    const next = new Set(collapsed);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    writePref("shard.collapsedGroups", JSON.stringify([...next]));
    setCollapsed(next);
  };

  // Избранное сверху, потом группы по алфавиту, потом сборки без группы
  const sections = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = instances
      .filter((i) => !q || i.name.toLowerCase().includes(q) || i.mcVersion.includes(q) || i.group?.toLowerCase().includes(q))
      .toSorted(comparators[sort]);
    const out: { key: string; title: string | null; items: Instance[] }[] = [];
    const favorites = list.filter((i) => i.favorite);
    if (favorites.length) out.push({ key: "★", title: t("groups.favorites"), items: favorites });
    const groups = [...new Set(list.flatMap((i) => (!i.favorite && i.group ? [i.group] : [])))].sort((a, b) => a.localeCompare(b));
    for (const g of groups) out.push({ key: g, title: g, items: list.filter((i) => !i.favorite && i.group === g) });
    const rest = list.filter((i) => !i.favorite && !i.group);
    if (rest.length) out.push({ key: "", title: out.length ? t("groups.ungrouped") : null, items: rest });
    return out;
  }, [instances, query, sort, t]);

  return (
    <Page
      title={t("instances.title")}
      actions={
        <>
          <Button variant="ghost" onClick={() => openImport()}>
            <Download /> {t("instances.import")}
          </Button>
          <Button onClick={() => setCreating(true)}>
            <Plus /> {t("instances.create")}
          </Button>
        </>
      }
    >
      {instances.length === 0 ? (
        <EmptyState
          icon={<Boxes className="size-8" />}
          title={t("home.emptyTitle")}
          text={t("home.emptyText")}
          action={
            <Button onClick={() => setCreating(true)}>
              <Plus /> {t("home.create")}
            </Button>
          }
        />
      ) : (
        <>
          <div className="flex gap-2">
            <label className="flex h-9 flex-1 items-center gap-2 rounded-md border border-surface-3 bg-surface-1 px-3 focus-within:border-ink-muted">
              <Search className="size-3.5 text-ink-muted" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("instances.search")}
                className="h-full flex-1 bg-transparent text-[13px] outline-none placeholder:text-ink-muted"
              />
            </label>
            <span className="relative">
              <select
                value={sort}
                onChange={(e) => setSort(e.target.value as Sort)}
                aria-label={t("instances.sortLabel")}
                className="h-9 appearance-none rounded-md border border-surface-3 bg-surface-1 pr-8 pl-3 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                {sorts.map((s) => (
                  <option key={s} value={s}>
                    {t(`instances.sort.${s}`)}
                  </option>
                ))}
              </select>
              <ChevronDown className="pointer-events-none absolute top-1/2 right-2.5 size-3.5 -translate-y-1/2 text-ink-muted" />
            </span>
          </div>

          {sections.length === 0 && <p className="text-[13px] text-ink-muted">{t("instances.nothingFound")}</p>}

          {sections.map((section) => {
            const isCollapsed = collapsed.has(section.key);
            return (
              <section key={section.key} className="flex flex-col gap-2">
                {section.title && (
                  <button
                    type="button"
                    onClick={() => toggleGroup(section.key)}
                    aria-expanded={!isCollapsed}
                    className="flex items-center gap-1.5 self-start text-[13px] text-ink-muted outline-none hover:text-foreground focus-visible:text-foreground"
                  >
                    {isCollapsed ? <ChevronRight className="size-3.5" /> : <ChevronDown className="size-3.5" />}
                    {section.key === "★" && <Star className="size-3.5 fill-current text-[#e0b040]" />}
                    {section.title}
                    <span className="text-xs tabular-nums">{section.items.length}</span>
                  </button>
                )}
                {!isCollapsed && (
                  <ul className="grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-3">
                    {section.items.map((inst) => (
                      <InstanceCard key={inst.id} inst={inst} />
                    ))}
                  </ul>
                )}
              </section>
            );
          })}
          <p className="text-xs text-ink-muted">{t("instances.contextHint")}</p>
        </>
      )}
      <CreateInstanceDialog open={creating} onOpenChange={setCreating} />
    </Page>
  );
}

function InstanceCard({ inst }: { inst: Instance }) {
  const { t, i18n } = useTranslation();
  const playtimes = usePlaytimes();
  const label = useInstanceLabel();
  return (
    <InstanceContextMenu instance={inst}>
      <li className="group relative flex flex-col gap-3 rounded-lg border border-surface-3 bg-surface-1 p-4 transition-colors hover:border-ink-muted/60 data-[state=open]:border-ink-muted/60">
        <Link to={`/instances/${inst.id}`} className="flex min-w-0 items-start gap-3 outline-none focus-visible:underline">
          <InstanceIcon name={inst.name} loader={inst.loader} icon={inst.icon} className="size-14 shrink-0 [--step:3px]" />
          <span className="min-w-0 flex-1 pr-6">
            <span className="block truncate text-foreground">{inst.name}</span>
            <span className="block truncate text-xs text-ink-muted">{label(inst)}</span>
            <span className="mt-1 flex items-center gap-1.5 text-xs text-ink-muted">
              {inst.modCount > 0 && <span className="truncate">{t("instances.mods", { count: inst.modCount })}</span>}
              <VersionTypeBadge type={inst.versionType} />
            </span>
          </span>
        </Link>
        <div className="mt-auto flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-[11px] text-ink-muted">
            {inst.lastPlayed ? t("instances.played", { when: timeAgo(inst.lastPlayed, i18n.language) }) : t("instances.neverPlayed")}
            {playtimes[inst.id] ? ` · ${formatPlaytime(playtimes[inst.id], t)}` : ""}
          </span>
          <PlayButton instanceId={inst.id} instanceName={inst.name} size="sm" />
        </div>
        <button
          type="button"
          onClick={() => void setInstanceMeta(inst.id, inst.group ?? null, !inst.favorite)}
          title={inst.favorite ? t("groups.unfavorite") : t("groups.favorite")}
          aria-label={inst.favorite ? t("groups.unfavorite") : t("groups.favorite")}
          aria-pressed={!!inst.favorite}
          className={cn(
            "absolute top-2 right-2 grid size-8 place-items-center rounded-md outline-none transition-opacity hover:bg-surface-2 focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring",
            inst.favorite ? "text-[#e0b040]" : "text-ink-muted opacity-0 group-hover:opacity-100",
          )}
        >
          <Star className={cn("size-4", inst.favorite && "fill-current")} />
        </button>
      </li>
    </InstanceContextMenu>
  );
}

const tabs = ["content", "worlds", "screenshots", "logs", "settings"] as const;
type Tab = (typeof tabs)[number];

export function InstancePage() {
  const { t } = useTranslation();
  const details = useInstanceDetails();
  const { id } = useParams();
  const [params, setParams] = useSearchParams();
  const instances = useInstances();
  const inst = instances.find((i) => i.id === id);
  const launchState = useLaunch(id ?? "");
  const tabParam = params.get("tab") as Tab | null;
  const tab: Tab = tabParam && tabs.includes(tabParam) ? tabParam : "content";
  const setTab = (next: Tab) => setParams(next === "content" ? {} : { tab: next }, { replace: true });

  // Файлы, брошенные в окно на этой странице, попадают в сборку
  useEffect(() => {
    if (!inst) return;
    setDropTarget({ id: inst.id, name: inst.name });
    return () => setDropTarget(null);
  }, [inst?.id, inst?.name]);

  const packUpdate = usePackUpdate(inst);
  if (!inst) return instances.length > 0 ? <Navigate to="/instances" replace /> : null;
  const idle = launchState.status === "idle";
  const server = inst.settings?.server;

  return (
    <Page className="h-full">
      <div className="flex items-center gap-4">
        <button
          type="button"
          title={t("icon.change")}
          aria-label={t("icon.change")}
          onClick={() => openInstanceDialog("icon", inst.id)}
          className="rounded-md outline-none transition-opacity hover:opacity-80 focus-visible:ring-2 focus-visible:ring-ring"
        >
          <InstanceIcon name={inst.name} loader={inst.loader} icon={inst.icon} className="size-14 [--step:3px]" />
        </button>
        <div className="min-w-0 flex-1">
          <h1 className="truncate font-display text-2xl font-medium tracking-tight">{inst.name}</h1>
          <p className="flex items-center gap-2 text-ink-muted">
            <span className="truncate">{details(inst)}</span>
            <VersionTypeBadge type={inst.versionType} />
            <PlaytimeButton instanceId={inst.id} name={inst.name} />
            {inst.pack?.versionName && (
              <button
                type="button"
                disabled={!idle}
                onClick={() => openChangeVersion(inst)}
                title={t("packVersion.menu")}
                className="flex shrink-0 items-center gap-1 rounded bg-surface-2 px-1.5 py-px text-xs outline-none hover:bg-surface-3 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
              >
                <History className="size-3" /> {inst.pack.versionName}
              </button>
            )}
          </p>
        </div>
        <Button
          variant="ghost"
          size="icon"
          title={t("instance.openFolder")}
          aria-label={t("instance.openFolder")}
          onClick={() => void backend.openInstanceDir(inst.id).catch((e) => showToast(errorText(e)))}
        >
          <FolderOpen />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          title={t("instance.delete")}
          aria-label={t("instance.delete")}
          disabled={!idle}
          onClick={() => openInstanceDialog("delete", inst.id)}
          className="hover:text-destructive"
        >
          <Trash2 />
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" title={t("instance.more")} aria-label={t("instance.more")}>
              <EllipsisVertical />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56 border-surface-3 bg-surface-1">
            <DropdownMenuItem onSelect={() => openInstanceDialog("clone", inst.id)}>
              <Copy /> {t("clone.menu")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => openInstanceDialog("export", inst.id)}>
              <PackageOpen /> {t("instance.export")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => openInstanceDialog("options", inst.id)}>
              <SlidersHorizontal /> {t("options.menu")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => openInstanceDialog("snapshots", inst.id)}>
              <Camera /> {t("snapshots.menu")}
            </DropdownMenuItem>
            {inst.pack && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuItem disabled={packUpdate.checking} onSelect={() => void packUpdate.check()}>
                  {packUpdate.checking ? <Loader2 className="animate-spin" /> : <RefreshCw />}
                  {packUpdate.checking ? t("packUpdate.checking") : t("packUpdate.check")}
                </DropdownMenuItem>
                <DropdownMenuItem disabled={!idle} onSelect={() => openChangeVersion(inst)}>
                  <History /> {t("packUpdate.rollback")}
                </DropdownMenuItem>
              </>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
        <PackUpdateButton instance={inst} update={packUpdate.update} disabled={!idle} />
        {server && idle && (
          <Button variant="secondary" onClick={() => void launch(inst.id, inst.name, true)} title={server}>
            <Server /> {t("server.joinShort")}
          </Button>
        )}
        <PlayButton instanceId={inst.id} instanceName={inst.name} />
      </div>

      <div role="tablist" className="flex gap-1 border-b border-surface-3">
        {tabs.map((key) => (
          <button
            key={key}
            role="tab"
            aria-selected={tab === key}
            onClick={() => setTab(key)}
            className={cn(
              "-mb-px border-b-2 px-3 py-2 text-[13px] outline-none transition-colors focus-visible:text-foreground",
              tab === key ? "border-foreground text-foreground" : "border-transparent text-ink-muted hover:text-foreground",
            )}
          >
            {t(`instance.tab.${key}`)}
          </button>
        ))}
      </div>

      {tab === "content" ? (
        <InstanceContent instance={inst} locked={!idle} />
      ) : tab === "worlds" ? (
        <WorldsTab instance={inst} />
      ) : tab === "screenshots" ? (
        <ScreenshotsTab instance={inst} />
      ) : tab === "logs" ? (
        <GameConsole instanceId={inst.id} />
      ) : (
        <InstanceSettingsForm instance={inst} disabled={!idle} />
      )}
    </Page>
  );
}

function GameConsole({ instanceId }: { instanceId: string }) {
  const { t } = useTranslation();
  const { logs, status, exitCode, error } = useLaunch(instanceId);
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  // Прокручиваем вниз, только если пользователь сам не отмотал вверх
  useEffect(() => {
    const el = scroller.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [logs.length]);

  const footer =
    error ??
    (status === "idle" && exitCode !== undefined
      ? exitCode === 0 || exitCode === null
        ? t("instance.exited")
        : t("instance.crashed", { code: exitCode })
      : null);

  return (
    <section className="flex min-h-[260px] flex-1 flex-col overflow-hidden rounded-lg border border-surface-3 bg-surface-1">
      <div className="flex items-center gap-2 border-b border-surface-3 px-4 py-1.5">
        <h2 className="flex-1 text-[13px] text-ink-muted">{t("instance.console")}</h2>
        {/* Пусто в этом запуске — отправится logs/latest.log с диска */}
        <ShareLogButton content={() => logs.map((l) => l.line).join("\n")} instanceId={instanceId} />
      </div>
      <div
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
        className="min-h-0 flex-1 overflow-y-auto px-4 py-3 font-mono text-[12px] leading-relaxed select-text"
      >
        {logs.length === 0 ? (
          <p className="font-sans text-[13px] text-ink-muted">{t("instance.consoleEmpty")}</p>
        ) : (
          logs.map((entry, i) => (
            <div
              key={i}
              className={cn(
                "break-all whitespace-pre-wrap",
                entry.stderr || /\/(ERROR|FATAL)\]/.test(entry.line) ? "text-destructive" : /\/WARN\]/.test(entry.line) ? "text-[#e0951a]" : "text-ink",
              )}
            >
              {entry.line}
            </div>
          ))
        )}
      </div>
      {footer && (
        <p className={cn("border-t border-surface-3 px-4 py-2 text-[13px]", error || (exitCode ?? 0) !== 0 ? "text-destructive" : "text-ink-muted")}>
          {footer}
        </p>
      )}
    </section>
  );
}

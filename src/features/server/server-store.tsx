import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, Check, Download, FolderOpen, Loader2, Plus, Puzzle, RefreshCw, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ProjectSearch } from "@/components/project-search";
import { ProjectIcon } from "@/components/project-icon";
import { showToast } from "@/components/toast";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { serverApi, storeApi, type ModsResult, type ServerConfig, type StoreItem, type StoreKind } from "./api";
import { Card, ModsReport } from "./server-tab";

/**
 * Контент сервера, как моды в сборке: список установленного и поиск с фильтрами и страницами.
 * Плагины (Paper, Purpur) или серверные моды, плюс ресурспак, который игроки скачивают при входе.
 */
export function ServerStore({ id, config, running, onSynced }: { id: string; config: ServerConfig; running: boolean; onSynced: () => void }) {
  const { t } = useTranslation();
  const plugins = config.core === "paper" || config.core === "purpur";
  const hasAddons = config.core !== "vanilla";
  const kinds: StoreKind[] = hasAddons ? ["addon", "resourcepack"] : ["resourcepack"];
  const [kind, setKind] = useState<StoreKind>(kinds[0]);
  const [browsing, setBrowsing] = useState(false);
  const [items, setItems] = useState<StoreItem[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [anyVersion, setAnyVersion] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [synced, setSynced] = useState<ModsResult | null>(null);

  const reload = () => void storeApi.list(id).then((l) => setItems(l.items), () => setItems([]));
  useEffect(reload, [id]);

  const installed = useMemo(() => new Set(items?.filter((i) => i.kind === kind).flatMap((i) => (i.projectId ? [i.projectId] : []))), [items, kind]);
  const list = items?.filter((i) => i.kind === kind) ?? null;
  const kindName = (k: StoreKind) => (k === "resourcepack" ? t("serverStore.kinds.resourcepack") : t(plugins ? "serverStore.kinds.plugins" : "serverStore.kinds.mods"));
  const projectType = kind === "resourcepack" ? "resourcepack" : plugins ? "plugin" : "mod";

  const install = async (projectId: string, title: string) => {
    setBusy(projectId);
    try {
      const names = await storeApi.install(id, kind, projectId, anyVersion);
      showToast(names.length > 1 ? t("serverStore.installedWithDeps", { name: title, count: names.length - 1 }) : t("serverStore.installedOne", { name: title }));
      reload();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const remove = async (item: StoreItem) => {
    try {
      await storeApi.remove(id, item.file);
      reload();
    } catch (e) {
      showToast(errorText(e));
    }
  };

  const sync = async () => {
    setSyncing(true);
    try {
      setSynced(await serverApi.syncMods(id));
      reload();
      onSynced();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setSyncing(false);
    }
  };

  const tabs = (
    <div role="tablist" className="flex gap-1">
      {kinds.map((k) => (
        <button
          key={k}
          role="tab"
          type="button"
          aria-selected={kind === k}
          onClick={() => setKind(k)}
          className={cn(
            "h-8 rounded-md px-3 text-[13px] text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
            kind === k && "bg-surface-2 text-foreground",
          )}
        >
          {kindName(k)}
        </button>
      ))}
    </div>
  );

  if (browsing) {
    return (
      <ProjectSearch
        resetKey={`${id}/${kind}`}
        projectType={projectType}
        sources={["modrinth"]}
        filters={{ baseVersion: config.mcVersion, isInstalled: (hit) => installed.has(hit.projectId) }}
        details={{ gameVersion: config.mcVersion }}
        search={(q) => {
          setAnyVersion(q.version !== null);
          return storeApi.search(id, kind, q.query, q.sort, q.offset, q.categories, q.version);
        }}
        placeholder={() => t(`serverStore.searchFor.${projectType}`, { version: config.mcVersion })}
        toolbar={
          <>
            <Button variant="ghost" size="sm" onClick={() => setBrowsing(false)} className="text-ink-muted">
              <ArrowLeft /> {t("content.backToList")}
            </Button>
            {tabs}
            <span className="flex-1" />
          </>
        }
        notice={
          <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
            <p className="text-xs text-ink-muted">
              {kind === "resourcepack" ? t("serverStore.packHint") : running ? t("serverStore.restartHint") : t("serverStore.forVersion", { version: config.mcVersion })}
            </p>
          </div>
        }
        action={(hit) => {
          const has = installed.has(hit.projectId);
          const loading = busy === hit.projectId;
          return (
            <Button
              size="sm"
              variant={has ? "ghost" : "secondary"}
              disabled={has || busy !== null}
              onClick={() => void install(hit.projectId, hit.title)}
              className="mt-1 w-[124px] shrink-0"
            >
              {loading ? <Loader2 className="animate-spin" /> : has ? <Check /> : <Download />}
              {loading ? t("content.installing") : has ? t("content.isInstalled") : kind === "resourcepack" ? t("serverStore.setPack") : t("content.install")}
            </Button>
          );
        }}
      />
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        {tabs}
        <span className="flex-1" />
        {kind === "addon" && (
          <Button variant="ghost" size="sm" onClick={() => void serverApi.openFolder(id, plugins ? "plugins" : "mods").catch((e) => showToast(errorText(e)))}>
            <FolderOpen /> {t(plugins ? "hosting.pluginsFolder" : "hosting.modsFolder")}
          </Button>
        )}
        <Button size="sm" onClick={() => setBrowsing(true)}>
          <Plus /> {kind === "resourcepack" ? t("serverStore.findPack") : t(plugins ? "serverStore.findPlugins" : "serverStore.findMods")}
        </Button>
      </div>
      {running && kind === "addon" && <p className="text-xs text-ink-muted">{t("serverStore.restartHint")}</p>}
      {kind === "resourcepack" && <p className="text-xs text-ink-muted">{t("serverStore.packHint")}</p>}

      {kind === "addon" && config.source && !plugins && (
        <Card>
          <p className="text-sm text-ink-muted">{t("hosting.modsText")}</p>
          {synced && <ModsReport result={synced} />}
          <div>
            <Button variant="secondary" disabled={syncing || running} onClick={() => void sync()}>
              {syncing ? <Loader2 className="animate-spin" /> : <RefreshCw />} {t("hosting.syncMods")}
            </Button>
          </div>
        </Card>
      )}

      {list === null ? (
        <Loader2 className="my-6 size-5 animate-spin text-ink-muted" />
      ) : list.length === 0 ? (
        <div className="flex flex-col items-start gap-2 rounded-lg border border-dashed border-surface-3 px-6 py-8">
          <Puzzle className="size-6 text-brand" />
          <p className="text-sm text-ink-muted">{kind === "resourcepack" ? t("serverStore.noPack") : t(plugins ? "serverStore.noPlugins" : "serverStore.noMods")}</p>
        </div>
      ) : (
        <ul className="flex flex-col divide-y divide-surface-3 rounded-lg border border-surface-3 bg-surface-1">
          {list.map((item) => {
            const locked = running && item.kind === "addon";
            return (
              <li key={item.file} className="flex items-center gap-3 px-4 py-2.5">
                <ProjectIcon url={item.iconUrl} className="size-10" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13.5px] text-foreground">{item.title}</p>
                  <p className="truncate font-mono text-[11px] text-ink-muted">{item.versionNumber ?? (item.kind === "addon" ? item.file : "")}</p>
                </div>
                <Button
                  variant="ghost"
                  size="icon"
                  disabled={locked}
                  title={locked ? t("hosting.stopToEdit") : t("serverStore.remove")}
                  aria-label={t("serverStore.remove")}
                  onClick={() => void remove(item)}
                >
                  <Trash2 />
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

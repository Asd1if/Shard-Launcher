import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Copy,
  FolderOpen,
  Globe,
  Hammer,
  Link2,
  Loader2,
  Package,
  Play,
  RefreshCw,
  Rocket,
  Router,
  Square,
  Trash2,
  TriangleAlert,
  Unlink,
  Users,
  Wifi,
  Plus,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ProgressBar } from "@/components/ui/progress-bar";
import { showToast } from "@/components/toast";
import { backend, isTauri, type World } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { fetchVersions } from "@/features/versions/api";
import { LoaderVersionSelect } from "@/features/instances/loader-version-select";
import { MemoryPresets } from "@/features/instances/instance-settings";
import { useInstances } from "@/features/instances/store";
import type { Instance, ModLoader } from "@/features/instances/types";
import { cores, moddedCores, serverApi, tunnelApi, type Core, type CreateProgress, type ModsResult, type ServerConfig, type ServerInfo } from "./api";
import { useServer } from "./store";
import { serverIconFor } from "./server-icon";
import { ServerPlayers } from "./server-players";
import { ServerConsole } from "./server-console";
import { ServerFiles } from "./server-files";
import { ServerAutomation, ServerBackups, ServerLoad } from "./server-load";
import { ServerStore } from "./server-store";

const select =
  "h-9 min-w-[180px] appearance-none rounded-md border border-input bg-surface-0 pr-8 pl-3 text-[13px] outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:opacity-50";

const openFolder = (id: string, sub?: string) => void serverApi.openFolder(id, sub).catch((e) => showToast(errorText(e)));

function copy(text: string, done: string) {
  void navigator.clipboard.writeText(text).then(() => showToast(done));
}

/** Ядро, которое подходит сборке: её загрузчик или ваниль. */
function coreOf(instance: Instance): Core {
  return (instance.loader as Core | undefined) ?? "vanilla";
}

// ---------- создание ----------

/**
 * Создание сервера или пересборка существующего (`existing`). Сервер самостоятельный:
 * сборка — лишь необязательный источник модов, мира или серверного архива модпака.
 */
export function CreateServer({
  existing,
  onCancel,
  onDone,
}: {
  existing?: { id: string; config: ServerConfig };
  onCancel?: () => void;
  onDone: (id: string) => void;
}) {
  const { t } = useTranslation();
  const instances = useInstances();
  const [name, setName] = useState(existing?.config.name ?? "");
  const [mode, setMode] = useState<"custom" | "instance" | "pack">("custom");
  const custom = mode === "custom";
  const [sourceId, setSourceId] = useState(existing?.config.source ?? "");
  const instance = instances.find((i) => i.id === sourceId);
  // Серверная версия модпака: null — ещё проверяем, false — её нет
  const [packAvailable, setPackAvailable] = useState<boolean | null>(false);
  const [core, setCore] = useState<Core>(existing?.config.core ?? "paper");
  const [mc, setMc] = useState(existing?.config.mcVersion ?? "");
  const [loaderVersion, setLoaderVersion] = useState<string | null>(existing?.config.loaderVersion ?? null);
  const [versions, setVersions] = useState<string[] | null>(null);
  const [worlds, setWorlds] = useState<World[]>([]);
  const [world, setWorld] = useState("");
  const [copyMods, setCopyMods] = useState(true);
  const [eula, setEula] = useState(false);
  const [progress, setProgress] = useState<CreateProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<{ id: string; mods: ModsResult } | null>(null);

  useEffect(() => {
    setPackAvailable(instance?.pack ? null : false);
    setWorld("");
    if (!instance) return setWorlds([]);
    void backend.worlds(instance.id).then(setWorlds, () => setWorlds([]));
    if (!instance.pack) return;
    void serverApi.packAvailable(instance.id).then(setPackAvailable, () => setPackAvailable(false));
  }, [instance?.id]);

  // Без сборки — только своё ядро; модпак — только у сборок из модпака
  useEffect(() => {
    if (!instance && mode !== "custom") setMode("custom");
    if (mode === "pack" && packAvailable === false) setMode("instance");
  }, [instance?.id, packAvailable]);

  const fromInstance = mode !== "custom" && !!instance;
  const effectiveCore = fromInstance ? coreOf(instance) : core;
  const effectiveMc = fromInstance ? instance.mcVersion : mc;
  const modded = moddedCores.includes(effectiveCore);
  const sameLoader = !!instance && modded && (effectiveCore === instance.loader || (effectiveCore === "quilt" && instance.loader === "fabric"));

  useEffect(() => {
    if (!custom) return;
    let alive = true;
    setVersions(null);
    void serverApi
      .coreVersions(core)
      .then(async (list) => list ?? (await fetchVersions()).versions.filter((v) => v.type === "release").map((v) => v.id))
      .then((list) => {
        if (!alive) return;
        setVersions(list);
        if (!list.includes(mc) && list[0]) setMc(list[0]);
      })
      .catch(() => alive && setVersions(mc ? [mc] : []));
    return () => {
      alive = false;
    };
  }, [core, custom]);

  // Иконка сборки — иконкой сервера в списке серверов игры
  const setIcon = (id: string) =>
    instance
      ? serverIconFor(instance)
          .then((png) => (png ? serverApi.setIcon(id, png) : undefined))
          .catch(() => {})
      : Promise.resolve();

  const create = async () => {
    setBusy(true);
    setProgress(null);
    try {
      if (mode === "pack" && instance) {
        const result = await serverApi.createFromPack(existing?.id ?? null, instance.id, name, eula, setProgress);
        await setIcon(result.id);
        showToast(t("hosting.created"));
        onDone(result.id);
        return;
      }
      let lv = fromInstance ? (instance.loaderVersion ?? null) : loaderVersion;
      if (modded && !lv) {
        // «Рекомендованная» у сборки — узнаём, какая именно
        const list = await backend.loaderVersions(effectiveCore as ModLoader, effectiveMc);
        lv = (list.find((v) => v.recommended) ?? list.find((v) => v.stable) ?? list[0])?.version ?? null;
      }
      const result = await serverApi.create(
        existing?.id ?? null,
        {
          name: name.trim() || (fromInstance ? instance.name : `${t(`hosting.core.${effectiveCore}`)} ${effectiveMc}`),
          core: effectiveCore,
          mcVersion: effectiveMc,
          loaderVersion: modded ? lv : null,
          source: fromInstance ? instance.id : null,
          copyMods: copyMods && sameLoader && fromInstance,
          world: (fromInstance && world) || null,
          eula,
        },
        setProgress,
      );
      await setIcon(result.id);
      showToast(t("hosting.created"));
      if (result.mods?.skipped.length) setDone({ id: result.id, mods: result.mods });
      else onDone(result.id);
    } catch (e) {
      showToast(t("hosting.createFailed", { error: errorText(e) }));
    } finally {
      setBusy(false);
    }
  };

  if (done && !busy) {
    return (
      <Card>
        <h2 className="text-foreground">{t("hosting.created")}</h2>
        <ModsReport result={done.mods} />
        <div>
          <Button onClick={() => onDone(done.id)}>{t("hosting.toServer")}</Button>
        </div>
      </Card>
    );
  }

  return (
    <div className="flex flex-col gap-3 pb-6">
      <Card>
        <div>
          <h2 className="font-display text-base text-foreground">{existing ? t("hosting.rebuildTitle") : t("hosting.createTitle")}</h2>
          <p className="text-sm text-ink-muted">{t("hosting.createText")}</p>
        </div>

        <div className="flex flex-wrap items-end gap-3">
          <label className="flex min-w-[220px] flex-1 flex-col gap-1 text-xs text-ink-muted">
            {t("hosting.name")}
            <Input value={name} maxLength={64} onChange={(e) => setName(e.target.value)} placeholder={t("hosting.namePlaceholder")} className="text-[13px]" />
          </label>
          <label className="flex flex-col gap-1 text-xs text-ink-muted">
            {t("hosting.source")}
            <select value={sourceId} onChange={(e) => setSourceId(e.target.value)} className={select}>
              <option value="">{t("hosting.sourceNone")}</option>
              {instances.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.name}
                </option>
              ))}
            </select>
          </label>
        </div>

        {instance && (
          <div role="radiogroup" className={cn("grid gap-2", instance.pack ? "sm:grid-cols-3" : "sm:grid-cols-2")}>
            {instance.pack && (
              <Choice
                active={mode === "pack"}
                disabled={!packAvailable}
                onClick={() => setMode("pack")}
                icon={packAvailable === null ? <Loader2 className="animate-spin" /> : <Rocket />}
                title={t("hosting.fromPack")}
              >
                {packAvailable === null
                  ? t("hosting.packChecking")
                  : packAvailable
                    ? t(instance.pack.source === "curseforge" ? "hosting.packCurseforge" : "hosting.packModrinth")
                    : t("hosting.packMissing")}
              </Choice>
            )}
            <Choice active={mode === "instance"} onClick={() => setMode("instance")} icon={<Package />} title={t("hosting.fromInstance")}>
              {t(`hosting.core.${coreOf(instance)}`)} {instance.mcVersion}
              {instance.loader ? ` · ${t("hosting.withMods")}` : ""}
            </Choice>
            <Choice active={custom} onClick={() => setMode("custom")} icon={<Hammer />} title={t("hosting.custom")}>
              {t("hosting.customText")}
            </Choice>
          </div>
        )}

        {custom && (
          <div className="flex flex-col gap-3">
            <div role="radiogroup" aria-label={t("hosting.coreLabel")} className="flex flex-wrap gap-1.5">
              {cores.map((c) => (
                <button
                  key={c}
                  type="button"
                  role="radio"
                  aria-checked={core === c}
                  onClick={() => setCore(c)}
                  className={cn(
                    "rounded-md border px-3 py-1.5 text-[13px] transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    core === c ? "border-foreground bg-foreground text-background" : "border-surface-3 text-ink-muted hover:text-foreground",
                  )}
                >
                  {t(`hosting.core.${c}`)}
                </button>
              ))}
            </div>
            <p className="text-xs text-ink-muted">{t(`hosting.coreHint.${core}`)}</p>
            <div className="flex flex-wrap items-center gap-3">
              <label className="flex flex-col gap-1 text-xs text-ink-muted">
                {t("hosting.mcVersion")}
                <select value={mc} onChange={(e) => setMc(e.target.value)} disabled={!versions} className={select}>
                  {(versions ?? [mc]).map((v) => (
                    <option key={v} value={v}>
                      {v}
                    </option>
                  ))}
                </select>
              </label>
              {modded && mc && (
                <label className="flex flex-col gap-1 text-xs text-ink-muted">
                  {t("hosting.loaderVersion")}
                  <LoaderVersionSelect loader={core as ModLoader} mcVersion={mc} value={loaderVersion} onChange={setLoaderVersion} />
                </label>
              )}
            </div>
          </div>
        )}

        <div className="flex flex-col gap-2">
          {fromInstance && mode !== "pack" && (
            <label className="flex flex-col gap-1 text-xs text-ink-muted">
              {t("hosting.world")}
              <select value={world} onChange={(e) => setWorld(e.target.value)} className={cn(select, "w-fit")}>
                <option value="">{existing ? t("hosting.worldKeep") : t("hosting.worldNew")}</option>
                {worlds.map((w) => (
                  <option key={w.folder} value={w.folder}>
                    {t("hosting.worldFrom", { name: w.folder })}
                  </option>
                ))}
              </select>
            </label>
          )}
          {fromInstance && sameLoader && mode !== "pack" && (
            <Check checked={copyMods} onChange={setCopyMods}>
              {t("hosting.copyMods")}
            </Check>
          )}
          <Check checked={eula} onChange={setEula}>
            {t("hosting.eulaBefore")}{" "}
            <button
              type="button"
              className="text-foreground underline underline-offset-2"
              onClick={() => void (isTauri ? openUrl("https://aka.ms/MinecraftEULA") : window.open("https://aka.ms/MinecraftEULA"))}
            >
              {t("hosting.eulaLink")}
            </button>
          </Check>
        </div>

        {busy && (
          <ProgressBar
            unit={progress?.stage === "packDownload" ? "bytes" : "count"}
            label={t(`hosting.stage.${progress?.stage ?? "java"}`)}
            progress={progress && progress.total > 1 ? { done: progress.done, total: progress.total } : null}
          />
        )}

        <div className="flex gap-2">
          <Button disabled={!eula || busy || (custom && !mc)} onClick={() => void create()}>
            {busy ? <Loader2 className="animate-spin" /> : mode === "pack" ? <Rocket /> : <Hammer />}{" "}
            {mode === "pack" ? t("hosting.raisePack") : existing ? t("hosting.rebuild") : t("hosting.create")}
          </Button>
          {onCancel && (
            <Button variant="ghost" disabled={busy} onClick={onCancel}>
              {t("create.cancel")}
            </Button>
          )}
        </div>
      </Card>
    </div>
  );
}

export function ModsReport({ result }: { result: ModsResult }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-1 text-sm">
      <p className="text-ink">{t("hosting.modsCopied", { count: result.copied })}</p>
      {result.skipped.length > 0 && (
        <>
          <p className="text-ink-muted">{t("hosting.modsSkipped", { count: result.skipped.length })}</p>
          <ul className="flex flex-wrap gap-1">
            {result.skipped.map((m) => (
              <li key={m} className="rounded bg-surface-2 px-2 py-0.5 font-mono text-[11px] text-ink-muted">
                {m}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

// ---------- сервер ----------

type ServerState = ReturnType<typeof useServer>;
const panels = ["console", "players", "store", "files", "settings"] as const;

/** Страница сервера: запуск, адреса, консоль, магазин плагинов/модов и параметры. */
export function ServerView({ id, onDeleted }: { id: string; onDeleted: () => void }) {
  const server = useServer(id);
  const [rebuild, setRebuild] = useState(false);
  if (!server.info) return <Loader2 className="my-6 size-5 animate-spin text-ink-muted" />;
  const config = server.info.config;
  if (!config) return null;
  if (rebuild) {
    return (
      <CreateServer
        existing={{ id, config }}
        onCancel={() => setRebuild(false)}
        onDone={() => {
          setRebuild(false);
          server.reload();
        }}
      />
    );
  }
  return <ServerPanel id={id} server={server} onRebuild={() => setRebuild(true)} onDeleted={onDeleted} />;
}

function ServerPanel({ id, server, onRebuild, onDeleted }: { id: string; server: ServerState; onRebuild: () => void; onDeleted: () => void }) {
  const { t } = useTranslation();
  const info = server.info!;
  const config = info.config!;
  const [panel, setPanel] = useState<(typeof panels)[number]>("console");
  const [busy, setBusy] = useState(false);
  const live = info.status !== "stopped";
  const plugins = config.core === "paper" || config.core === "purpur";
  const hasStore = plugins || moddedCores.includes(config.core);

  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(false);
      server.reload();
    }
  };

  return (
    <div className="flex flex-col gap-3 pb-6">
      <Card className="flex-row flex-wrap items-center gap-4">
        <StatusDot status={info.status} />
        <div className="min-w-0 flex-1">
          <p className="text-foreground">
            {t(`hosting.core.${config.core}`)} {config.mcVersion}
            {config.loaderVersion && <span className="text-ink-muted"> · {config.loaderVersion}</span>}
            {config.fromPack && <span className="text-ink-muted"> · {t("hosting.packLabel", { name: config.fromPack })}</span>}
          </p>
          <p className="text-xs text-ink-muted">
            {t(`hosting.status.${info.status}`)}
            {info.status === "running" && ` · ${t("hosting.players", { count: info.players.length })}`}
          </p>
        </div>
        <Button variant="ghost" size="icon" title={t("hosting.openFolder")} aria-label={t("hosting.openFolder")} onClick={() => openFolder(id)}>
          <FolderOpen />
        </Button>
        {live ? (
          <>
            {info.status === "stopping" && (
              <Button variant="ghost" disabled={busy} onClick={() => void run(() => serverApi.stop(id, true))}>
                {t("hosting.kill")}
              </Button>
            )}
            <Button variant="secondary" disabled={busy || info.status === "stopping"} onClick={() => void run(() => serverApi.stop(id))}>
              <Square /> {t("hosting.stop")}
            </Button>
          </>
        ) : (
          <Button disabled={busy} onClick={() => void run(() => serverApi.start(id))}>
            {busy ? <Loader2 className="animate-spin" /> : <Play />} {t("hosting.start")}
          </Button>
        )}
      </Card>

      <Addresses id={id} info={info} onChange={server.reload} />
      <AddToGame id={id} info={info} />

      {info.status === "running" && <ServerLoad id={id} config={config} />}

      {info.players.length > 0 && (
        <p className="flex flex-wrap items-center gap-1.5 text-xs text-ink-muted">
          <Users className="size-3.5" />
          {info.players.map((p) => (
            <span key={p} className="rounded bg-surface-2 px-2 py-0.5 text-ink">
              {p}
            </span>
          ))}
        </p>
      )}

      <div role="tablist" className="flex gap-1 border-b border-surface-3">
        {panels
          .filter((p) => p !== "store" || hasStore)
          .map((p) => (
            <button
              key={p}
              role="tab"
              aria-selected={panel === p}
              onClick={() => setPanel(p)}
              className={cn(
                "-mb-px border-b-2 px-3 py-2 text-[13px] outline-none transition-colors focus-visible:text-foreground",
                panel === p ? "border-foreground text-foreground" : "border-transparent text-ink-muted hover:text-foreground",
              )}
            >
              {p === "store" ? t(plugins ? "hosting.panel.plugins" : "hosting.panel.mods") : t(`hosting.panel.${p}`)}
            </button>
          ))}
      </div>

      {panel === "console" ? (
        <ServerConsole id={id} server={server} />
      ) : panel === "files" ? (
        <ServerFiles id={id} running={live} />
      ) : panel === "players" ? (
        <ServerPlayers id={id} running={info.status === "running"} />
      ) : panel === "settings" ? (
        <ServerSettings id={id} info={info} disabled={live} onSaved={server.reload} onRebuild={onRebuild} onDeleted={onDeleted} />
      ) : (
        <ServerStore id={id} config={config} running={live} onSynced={server.reload} />
      )}
    </div>
  );
}

export function StatusDot({ status }: { status: ServerInfo["status"] }) {
  return (
    <span
      className={cn(
        "size-2.5 shrink-0 rounded-full",
        status === "running" ? "bg-[#3fb950]" : status === "stopped" ? "bg-surface-3" : "animate-pulse bg-[#e0951a]",
      )}
    />
  );
}

function Addresses({ id, info, onChange }: { id: string; info: ServerInfo; onChange: () => void }) {
  const { t } = useTranslation();
  const config = info.config!;
  const share = info.share;
  const [busy, setBusy] = useState(false);
  const copied = t("hosting.copied");

  const toggle = async () => {
    try {
      await serverApi.setConfig(id, { memoryMb: config.memoryMb, jvmArgs: config.jvmArgs, share: !config.share });
      onChange();
    } catch (e) {
      showToast(errorText(e));
    }
  };

  const reshare = async () => {
    setBusy(true);
    try {
      await serverApi.reshare(id);
      onChange();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <h3 className="text-foreground">{t("hosting.addressTitle")}</h3>
          <p className="text-sm text-ink-muted">{t("hosting.addressText")}</p>
        </div>
        <label className="flex cursor-pointer items-center gap-2 text-[13px] text-ink select-none">
          <Switch checked={config.share} onChange={() => void toggle()} label={t("hosting.shareToggle")} />
          {t("hosting.shareToggle")}
        </label>
      </div>

      <div className="flex flex-col gap-1.5">
        {info.lan && <AddressRow icon={<Wifi />} label={t("hosting.lan")} value={info.lan} copied={copied} />}
        {config.share && info.status !== "stopped" && !share && (
          <p className="flex items-center gap-2 text-xs text-ink-muted">
            <Loader2 className="size-3.5 animate-spin" /> {t("hosting.opening")}
          </p>
        )}
        {/* Два адреса: локальный — для своей сети, интернет — для всех. За CGNAT остаётся только IPv6 */}
        {share && (() => {
          const internet = share.public && (share.upnp || !share.cgnat) ? share.public : share.ipv6;
          return internet && <AddressRow icon={<Globe />} label={t("hosting.internet")} value={internet} copied={copied} highlight />;
        })()}
      </div>

      {config.share && info.status === "stopped" && <p className="text-xs text-ink-muted">{t("hosting.shareOnStart")}</p>}
      {share && (
        <div className="flex flex-col gap-1.5 text-xs">
          {share.upnp && <p className="flex items-center gap-1.5 text-[#3fb950]"><Router className="size-3.5" /> {t("hosting.upnpOk", { port: share.port })}</p>}
          {share.cgnat ? (
            <Warning>{share.ipv6 ? t("hosting.cgnatIpv6") : t("hosting.cgnat")}</Warning>
          ) : (
            !share.upnp && <Warning>{t(share.upnpError === "refused" ? "hosting.upnpRefused" : "hosting.upnpMissing", { port: share.port })}</Warning>
          )}
          <p className="text-ink-muted">{t("hosting.dynamicHint")}</p>
          <div>
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => void reshare()}>
              {busy ? <Loader2 className="animate-spin" /> : <RefreshCw />} {t("hosting.reshare")}
            </Button>
          </div>
        </div>
      )}
      {config.share && <p className="text-xs text-ink-muted">{t("hosting.firewall")}</p>}
      <Tunnel id={id} info={info} onChange={onChange} />
    </Card>
  );
}

/** Туннель playit.gg: постоянный адрес, даже без белого IP и UPnP */
function Tunnel({ id, info, onChange }: { id: string; info: ServerInfo; onChange: () => void }) {
  const { t } = useTranslation();
  const config = info.config!;
  const [linked, setLinked] = useState<boolean | null>(null);
  const [linking, setLinking] = useState(false);
  const copied = t("hosting.copied");

  useEffect(() => {
    void tunnelApi.status().then((s) => setLinked(s.linked), () => setLinked(false));
  }, []);

  const toggle = async (on: boolean) => {
    try {
      await serverApi.setTunnel(id, on);
      onChange();
    } catch (e) {
      showToast(errorText(e));
    }
  };

  const link = async () => {
    setLinking(true);
    try {
      const { code, url } = await tunnelApi.linkStart();
      if (isTauri) await openUrl(url);
      else window.open(url, "_blank");
      await tunnelApi.linkFinish(code);
      setLinked(true);
      showToast(t("hosting.tunnelLinked"));
      if (!config.tunnel) await toggle(true);
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setLinking(false);
    }
  };

  const unlink = async () => {
    try {
      if (config.tunnel) await serverApi.setTunnel(id, false);
      await tunnelApi.unlink();
      setLinked(false);
      onChange();
    } catch (e) {
      showToast(errorText(e));
    }
  };

  if (linked === null) return null;
  const pending = config.tunnel && info.status !== "stopped" && !info.tunnel && !info.tunnelError;

  return (
    <div className="flex flex-col gap-3 border-t border-surface-3 pt-3">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <h4 className="text-[13.5px] text-foreground">{t("hosting.tunnelTitle")}</h4>
          <p className="text-sm text-ink-muted">{t("hosting.tunnelText")}</p>
        </div>
        {linked && (
          <label className="flex cursor-pointer items-center gap-2 text-[13px] text-ink select-none">
            <Switch checked={config.tunnel} onChange={(v) => void toggle(v)} label={t("hosting.tunnelToggle")} />
            {t("hosting.tunnelToggle")}
          </label>
        )}
      </div>

      {!linked ? (
        <div className="flex flex-wrap items-center gap-3">
          <Button size="sm" disabled={linking} onClick={() => void link()}>
            {linking ? <Loader2 className="animate-spin" /> : <Link2 />} {t("hosting.tunnelLink")}
          </Button>
          {linking && <span className="text-xs text-ink-muted">{t("hosting.tunnelConfirm")}</span>}
        </div>
      ) : (
        <>
          {info.tunnel && <AddressRow icon={<Globe />} label={t("hosting.internet")} value={info.tunnel} copied={copied} highlight />}
          {pending && (
            <p className="flex items-center gap-2 text-xs text-ink-muted">
              <Loader2 className="size-3.5 animate-spin" /> {t("hosting.tunnelOpening")}
            </p>
          )}
          {config.tunnel && info.tunnelError && <Warning>{errorText(info.tunnelError)}</Warning>}
          {config.tunnel && info.status === "stopped" && <p className="text-xs text-ink-muted">{t("hosting.shareOnStart")}</p>}
          <div>
            <Button variant="ghost" size="sm" onClick={() => void unlink()}>
              <Unlink /> {t("hosting.tunnelUnlink")}
            </Button>
          </div>
        </>
      )}
    </div>
  );
}

function AddressRow({ icon, label, value, copied, highlight }: { icon: React.ReactNode; label: string; value: string; copied: string; highlight?: boolean }) {
  return (
    <div className={cn("flex items-center gap-3 rounded-md px-3 py-2", highlight ? "bg-brand/10" : "bg-surface-0")}>
      <span className="text-ink-muted [&>svg]:size-4">{icon}</span>
      <span className="w-24 shrink-0 text-xs text-ink-muted">{label}</span>
      <code className="min-w-0 flex-1 truncate font-mono text-[13px] text-foreground select-text">{value}</code>
      <Button variant="ghost" size="icon" aria-label={copied} title={copied} onClick={() => copy(value, copied)}>
        <Copy />
      </Button>
    </div>
  );
}

type Field = { key: string; kind: "text" | "number" | "bool" | "select"; options?: string[] };

/** Частые параметры server.properties по группам — остальные в «Все параметры». */
const GROUPS: { key: string; fields: Field[] }[] = [
  {
    key: "general",
    fields: [
      { key: "motd", kind: "text" },
      { key: "max-players", kind: "number" },
      { key: "server-port", kind: "number" },
    ],
  },
  {
    key: "gameplay",
    fields: [
      { key: "gamemode", kind: "select", options: ["survival", "creative", "adventure", "spectator"] },
      { key: "difficulty", kind: "select", options: ["peaceful", "easy", "normal", "hard"] },
      { key: "hardcore", kind: "bool" },
      { key: "pvp", kind: "bool" },
      { key: "allow-flight", kind: "bool" },
      { key: "enable-command-block", kind: "bool" },
    ],
  },
  {
    key: "access",
    fields: [
      { key: "online-mode", kind: "bool" },
      { key: "white-list", kind: "bool" },
      { key: "spawn-protection", kind: "number" },
    ],
  },
  {
    key: "world",
    fields: [
      { key: "level-seed", kind: "text" },
      { key: "allow-nether", kind: "bool" },
      { key: "view-distance", kind: "number" },
      { key: "simulation-distance", kind: "number" },
    ],
  },
];
const FIELDS = GROUPS.flatMap((g) => g.fields);

function ServerSettings({
  id,
  info,
  disabled,
  onSaved,
  onRebuild,
  onDeleted,
}: {
  id: string;
  info: ServerInfo;
  disabled: boolean;
  onSaved: () => void;
  onRebuild: () => void;
  onDeleted: () => void;
}) {
  const { t } = useTranslation();
  const config = info.config!;
  const [props, setProps] = useState<[string, string][] | null>(null);
  const [changed, setChanged] = useState<Record<string, string>>({});
  const [memory, setMemory] = useState(config.memoryMb);
  const [jvmArgs, setJvmArgs] = useState(config.jvmArgs);
  const [maxMemory, setMaxMemory] = useState(16384);
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState<{ done: number; total: number } | null>(null);

  useEffect(() => {
    void serverApi.properties(id).then(setProps, () => setProps([]));
    void backend.systemInfo().then((s) => setMaxMemory(s.totalMemoryMb));
  }, [id]);

  const value = (key: string) => changed[key] ?? props?.find(([k]) => k === key)?.[1] ?? "";
  const set = (key: string, v: string) => setChanged((c) => ({ ...c, [key]: v }));
  const dirty = Object.keys(changed).length > 0 || memory !== config.memoryMb || jvmArgs !== config.jvmArgs;
  const known = new Set(FIELDS.map((f) => f.key));

  const save = async () => {
    setSaving(true);
    try {
      if (Object.keys(changed).length) await serverApi.setProperties(id, changed);
      await serverApi.setConfig(id, { memoryMb: memory, jvmArgs, share: config.share });
      setProps(await serverApi.properties(id));
      setChanged({});
      onSaved();
      showToast(t("hosting.saved"));
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  if (!props) return <Loader2 className="my-6 size-5 animate-spin text-ink-muted" />;

  return (
    <div className="flex flex-col gap-3">
      <ServerAutomation id={id} config={config} onSaved={onSaved} />
      <ServerBackups id={id} running={info.status !== "stopped"} />
      {disabled && <p className="text-xs text-ink-muted">{t("hosting.stopToEdit")}</p>}
      <Card>
        <h3 className="text-foreground">{t("hosting.memory")}</h3>
        <div className="flex items-center gap-3">
          <input
            type="range"
            min={1024}
            max={Math.max(1024, maxMemory - 1024)}
            step={512}
            value={memory}
            disabled={disabled}
            onChange={(e) => setMemory(Number(e.target.value))}
            className="flex-1 accent-brand"
            aria-label={t("hosting.memory")}
          />
          <span className="w-16 text-right text-sm text-foreground tabular-nums">{(memory / 1024).toFixed(1).replace(/\.0$/, "")} GB</span>
        </div>
        <MemoryPresets value={memory} max={maxMemory} onChange={setMemory} />
        <label className="flex flex-col gap-1 text-xs text-ink-muted">
          {t("hosting.jvmArgs")}
          <Input value={jvmArgs} disabled={disabled} onChange={(e) => setJvmArgs(e.target.value)} placeholder="-XX:+UseG1GC" className="font-mono text-[13px]" />
        </label>
      </Card>

      {GROUPS.map((group) => (
        <Card key={group.key}>
          <h3 className="text-foreground">{t(`hosting.group.${group.key}`)}</h3>
          <div className="grid gap-x-6 gap-y-3 sm:grid-cols-2">
            {group.fields.map((f) => (
              <label key={f.key} className={cn("flex flex-col gap-1 text-xs text-ink-muted", f.kind === "bool" && "flex-row items-center justify-between gap-3")}>
                <span>
                  {t(`hosting.prop.${f.key}`)}
                  {f.key === "online-mode" && <span className="block text-[11px]">{t("hosting.onlineModeHint")}</span>}
                </span>
                {f.kind === "bool" ? (
                  <Switch checked={value(f.key) === "true"} disabled={disabled} onChange={(v) => set(f.key, String(v))} label={t(`hosting.prop.${f.key}`)} />
                ) : f.kind === "select" ? (
                  <select value={value(f.key)} disabled={disabled} onChange={(e) => set(f.key, e.target.value)} className={select}>
                    {f.options!.map((o) => (
                      <option key={o} value={o}>
                        {t(`hosting.option.${o}`)}
                      </option>
                    ))}
                  </select>
                ) : (
                  <Input
                    type={f.kind === "number" ? "number" : "text"}
                    value={value(f.key)}
                    disabled={disabled}
                    onChange={(e) => set(f.key, e.target.value)}
                    className="text-[13px]"
                  />
                )}
              </label>
            ))}
          </div>
        </Card>
      ))}

      {props.some(([k]) => !known.has(k)) && (
        <Card>
          <details className="text-[13px]">
            <summary className="cursor-pointer text-foreground select-none">{t("hosting.allProperties")}</summary>
            <div className="mt-3 grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-1.5">
              {props
                .filter(([k]) => !known.has(k))
                .map(([k]) => (
                  <label key={k} className="contents">
                    <span className="truncate self-center font-mono text-[12px] text-ink-muted">{k}</span>
                    <Input value={value(k)} disabled={disabled} onChange={(e) => set(k, e.target.value)} className="h-8 font-mono text-[12px]" />
                  </label>
                ))}
            </div>
          </details>
        </Card>
      )}

      <div className="flex flex-wrap gap-2">
        <Button disabled={!dirty || saving || disabled} onClick={() => void save()}>
          {saving && <Loader2 className="animate-spin" />} {t("hosting.save")}
        </Button>
        <span className="flex-1" />
        <Button variant="ghost" disabled={disabled} onClick={onRebuild}>
          <Hammer /> {t("hosting.rebuild")}
        </Button>
        {confirmDelete ? (
          <Button
            variant="destructive"
            disabled={disabled || !!deleting}
            onClick={() =>
              void (async () => {
                setDeleting({ done: 0, total: 0 });
                try {
                  await serverApi.remove(id, setDeleting);
                  showToast(t("hosting.deleted"));
                  onDeleted();
                } catch (e) {
                  showToast(errorText(e));
                } finally {
                  setDeleting(null);
                }
              })()
            }
          >
            <Trash2 /> {t("hosting.deleteConfirm")}
          </Button>
        ) : (
          <Button variant="ghost" disabled={disabled} onClick={() => setConfirmDelete(true)}>
            <Trash2 /> {t("hosting.delete")}
          </Button>
        )}
      </div>
      {deleting && <ProgressBar unit="count" progress={deleting.total ? deleting : null} label={t("progress.deleting")} />}
    </div>
  );
}

// ---------- мелочи ----------

export function Card({ children, className }: { children: React.ReactNode; className?: string }) {
  return <section className={cn("flex flex-col gap-3 rounded-lg border border-surface-3 bg-surface-1 px-5 py-4", className)}>{children}</section>;
}

function Choice({
  active,
  disabled,
  onClick,
  icon,
  title,
  children,
}: {
  active: boolean;
  disabled?: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex items-start gap-3 rounded-lg border px-4 py-3 text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
        active ? "border-foreground bg-surface-2" : "border-surface-3 hover:border-ink-muted",
      )}
    >
      <span className="mt-0.5 text-ink-muted [&>svg]:size-4">{icon}</span>
      <span className="min-w-0">
        <span className="block text-[13px] text-foreground">{title}</span>
        <span className="block text-xs text-ink-muted">{children}</span>
      </span>
    </button>
  );
}

function Check({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: React.ReactNode }) {
  return (
    <label className="flex cursor-pointer items-center gap-2 text-[13px] text-ink select-none">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} className="size-3.5 accent-brand" />
      <span>{children}</span>
    </label>
  );
}

function Switch({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative h-5 w-9 shrink-0 rounded-full border outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
        checked ? "border-foreground bg-foreground" : "border-surface-3 bg-surface-3",
      )}
    >
      <span className={cn("absolute top-1/2 size-3.5 -translate-y-1/2 rounded-full transition-all", checked ? "left-[18px] bg-background" : "left-[2px] bg-ink-muted")} />
    </button>
  );
}

function Warning({ children }: { children: React.ReactNode }) {
  return (
    <p className="flex items-start gap-1.5 rounded-md border border-[#e0951a]/30 bg-[#e0951a]/8 px-3 py-2 text-[#e0951a]">
      <TriangleAlert className="mt-0.5 size-3.5 shrink-0" /> <span>{children}</span>
    </p>
  );
}

/** Свой сервер — в список «Сетевая игра» выбранной сборки. */
function AddToGame({ id, info }: { id: string; info: ServerInfo }) {
  const { t } = useTranslation();
  const instances = useInstances();
  const [target, setTarget] = useState("");
  const [address, setAddress] = useState("local");
  const [busy, setBusy] = useState(false);
  const chosen = target || instances[0]?.id || "";
  if (!instances.length) return null;
  const options = [
    { key: "local", label: t("addToGame.local") },
    ...(info.lan ? [{ key: info.lan, label: `${t("hosting.lan")} · ${info.lan}` }] : []),
    ...(info.tunnel ? [{ key: info.tunnel, label: `playit.gg · ${info.tunnel}` }] : []),
  ];
  const add = async () => {
    setBusy(true);
    try {
      const added = await serverApi.addToInstance(id, chosen, address === "local" ? null : address);
      const name = instances.find((i) => i.id === chosen)?.name ?? "";
      showToast(added ? t("addToGame.added", { name }) : t("addToGame.already", { name }));
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const select = "h-8 min-w-0 rounded-md border border-surface-3 bg-surface-1 px-2 text-[13px] text-foreground";
  return (
    <Card>
      <div>
        <h3 className="text-foreground">{t("addToGame.title")}</h3>
        <p className="text-sm text-ink-muted">{t("addToGame.text")}</p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <select value={chosen} onChange={(e) => setTarget(e.target.value)} className={select} aria-label={t("addToGame.instance")}>
          {instances.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name}
            </option>
          ))}
        </select>
        <select value={address} onChange={(e) => setAddress(e.target.value)} className={select} aria-label={t("addToGame.address")}>
          {options.map((o) => (
            <option key={o.key} value={o.key}>
              {o.label}
            </option>
          ))}
        </select>
        <Button size="sm" variant="secondary" disabled={busy || !chosen} onClick={() => void add()}>
          {busy ? <Loader2 className="animate-spin" /> : <Plus />} {t("addToGame.add")}
        </Button>
      </div>
    </Card>
  );
}

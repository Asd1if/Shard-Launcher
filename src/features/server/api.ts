import { Channel, invoke } from "@tauri-apps/api/core";
import { isTauri } from "@/lib/backend";
import { contentApi, type SearchResult, type SearchSort } from "@/features/content/api";

export type Core = "vanilla" | "paper" | "purpur" | "fabric" | "quilt" | "forge" | "neoforge";
export const cores: Core[] = ["vanilla", "paper", "purpur", "fabric", "quilt", "forge", "neoforge"];
/** Ядра с загрузчиком модов — им нужна версия загрузчика */
export const moddedCores: Core[] = ["fabric", "quilt", "forge", "neoforge"];

export type ServerStatus = "stopped" | "starting" | "running" | "stopping";

export interface ServerConfig {
  name: string;
  core: Core;
  mcVersion: string;
  loaderVersion: string | null;
  memoryMb: number;
  jvmArgs: string;
  share: boolean;
  /** Туннель playit.gg при запуске */
  tunnel: boolean;
  launch: string[];
  javaComponent: string;
  copiedMods: string[];
  /** Версия модпака, с которой взят серверный набор */
  fromPack: string | null;
  /** Сборка, из которой брали моды */
  source: string | null;
  createdAt: string;
  autoRestart: boolean;
  autostart: boolean;
  /** Плановый перезапуск каждые N часов, 0 — нет */
  restartHours: number;
  /** Копия мира перед каждым запуском */
  backupOnStart: boolean;
}

export interface ServerBackup {
  file: string;
  createdAt: string;
  size: number;
}

export const serverBackupApi = {
  list: (id: string) => (isTauri ? invoke<ServerBackup[]>("server_backups", { id }) : Promise.resolve<ServerBackup[]>([])),
  make: (id: string) => (isTauri ? invoke<string>("server_backup", { id }) : Promise.reject(new Error("world_not_found"))),
  restore: (id: string, file: string) => (isTauri ? invoke<void>("server_backup_restore", { id, file }) : Promise.resolve()),
  remove: (id: string, file: string) => (isTauri ? invoke<void>("server_backup_delete", { id, file }) : Promise.resolve()),
};

export interface ServerStats {
  cpu: number;
  memoryMb: number;
  players: number;
  uptimeSecs: number;
}

export interface Share {
  port: number;
  upnp: boolean;
  upnpError: "no_gateway" | "refused" | null;
  public: string | null;
  cgnat: boolean;
  ipv6: string | null;
}

export interface ServerInfo {
  config: ServerConfig | null;
  status: ServerStatus;
  players: string[];
  port: number;
  share: Share | null;
  /** Адрес туннеля playit.gg и ошибка, если открыть не вышло */
  tunnel: string | null;
  tunnelError: string | null;
  lan: string | null;
}

export interface CreateArgs {
  name: string;
  core: Core;
  mcVersion: string;
  loaderVersion: string | null;
  /** Сборка — источник модов и мира; сервер к ней не привязывается */
  source: string | null;
  copyMods: boolean;
  world: string | null;
  eula: boolean;
}

export interface CreateProgress {
  stage: "pack" | "packDownload" | "java" | "core" | "install" | "world" | "mods";
  done: number;
  total: number;
}

export interface ModsResult {
  copied: number;
  skipped: string[];
}

export interface CreateResult {
  id: string;
  config: ServerConfig;
  mods: ModsResult | null;
}

export interface ServerEntry {
  id: string;
  config: ServerConfig;
}

// ---------- превью интерфейса без Rust: сервер понарошку ----------

const preview: Record<string, { info: ServerInfo; logs: string[]; props: [string, string][] }> = {};
const previewListeners = new Set<(id: string) => void>();
const previewEmit = (id: string) => previewListeners.forEach((fn) => fn(id));
export const onPreviewChange = (fn: (id: string) => void) => {
  previewListeners.add(fn);
  return () => void previewListeners.delete(fn);
};

function previewServer(id: string) {
  return (preview[id] ??= {
    info: { config: null, status: "stopped", players: [], port: 25565, share: null, tunnel: null, tunnelError: null, lan: "192.168.1.42:25565" },
    logs: [],
    props: [
      ["motd", "Preview — Shard"],
      ["server-port", "25565"],
      ["max-players", "20"],
      ["online-mode", "true"],
      ["pvp", "true"],
      ["difficulty", "easy"],
      ["gamemode", "survival"],
      ["white-list", "false"],
    ],
  });
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const serverApi = {
  info: (id: string) => (isTauri ? invoke<ServerInfo>("server_info", { id }) : Promise.resolve(structuredClone(previewServer(id).info))),

  logs: (id: string) => (isTauri ? invoke<string[]>("server_logs", { id }) : Promise.resolve([...previewServer(id).logs])),

  coreVersions: (core: Core) =>
    isTauri
      ? invoke<string[] | null>("server_core_versions", { core })
      : Promise.resolve(core === "paper" || core === "purpur" ? ["1.21.4", "1.21.3", "1.21.1", "1.20.6"] : null),

  list: () =>
    isTauri
      ? invoke<ServerEntry[]>("server_list")
      : Promise.resolve(Object.entries(preview).flatMap(([id, s]) => (s.info.config ? [{ id, config: structuredClone(s.info.config) }] : []))),

  rename: async (id: string, name: string) => {
    if (isTauri) return invoke<ServerConfig>("server_rename", { id, name });
    const s = previewServer(id);
    s.info.config = { ...s.info.config!, name };
    previewEmit(id);
    return s.info.config;
  },

  openFolder: (id: string, sub?: string) => (isTauri ? invoke<void>("server_open_folder", { id, sub: sub ?? null }) : Promise.resolve()),

  create: async (existing: string | null, args: CreateArgs, onProgress: (p: CreateProgress) => void) => {
    if (isTauri) {
      const progress = new Channel<CreateProgress>();
      progress.onmessage = onProgress;
      return invoke<CreateResult>("server_create", { id: existing, args, progress });
    }
    const id = existing ?? crypto.randomUUID();
    for (const stage of ["java", "core", "mods"] as const) {
      for (let i = 0; i <= 4; i++) {
        onProgress({ stage, done: i, total: 4 });
        await wait(120);
      }
    }
    const s = previewServer(id);
    const config: ServerConfig = {
      name: args.name,
      core: args.core,
      mcVersion: args.mcVersion,
      loaderVersion: args.loaderVersion,
      memoryMb: 4096,
      jvmArgs: "",
      share: false,
      tunnel: false,
      autoRestart: false,
      autostart: false,
      restartHours: 0,
      backupOnStart: false,
      launch: ["-jar", "server.jar", "nogui"],
      javaComponent: "java-runtime-delta",
      copiedMods: ["sodium-extra.jar"],
      fromPack: null,
      source: args.source,
      createdAt: new Date().toISOString(),
    };
    s.info.config = config;
    return { id, config, mods: args.copyMods ? { copied: 12, skipped: ["sodium-0.6.jar", "iris-1.8.jar"] } : null };
  },

  /** Есть ли у сборки серверная версия (CurseForge) или серверные файлы (Modrinth) */
  packAvailable: (id: string) => (isTauri ? invoke<boolean>("server_pack_available", { id }) : Promise.resolve(true)),

  createFromPack: async (existing: string | null, instanceId: string, name: string, eula: boolean, onProgress: (p: CreateProgress) => void) => {
    if (isTauri) {
      const progress = new Channel<CreateProgress>();
      progress.onmessage = onProgress;
      return invoke<CreateResult>("server_create_from_pack", { id: existing, instanceId, name: name.trim() || null, eula, progress });
    }
    const id = existing ?? crypto.randomUUID();
    for (const stage of ["pack", "java", "core"] as const) {
      for (let i = 0; i <= 4; i++) {
        onProgress({ stage, done: i, total: 4 });
        await wait(120);
      }
    }
    const s = previewServer(id);
    const config: ServerConfig = {
      name: name.trim() || "Preview Pack",
      core: "fabric",
      mcVersion: "1.21.1",
      loaderVersion: "0.16.10",
      memoryMb: 6144,
      jvmArgs: "",
      share: false,
      tunnel: false,
      autoRestart: false,
      autostart: false,
      restartHours: 0,
      backupOnStart: false,
      launch: ["-jar", "fabric-server-launch.jar", "nogui"],
      javaComponent: "java-runtime-delta",
      copiedMods: [],
      fromPack: "Preview Pack 1.2.0",
      source: instanceId,
      createdAt: new Date().toISOString(),
    };
    s.info.config = config;
    return { id, config, mods: null } as CreateResult;
  },

  /** png — data URL картинки 64×64 */
  setIcon: (id: string, png: string) => (isTauri ? invoke<void>("server_set_icon", { id, png }) : Promise.resolve()),

  syncMods: (id: string) =>
    isTauri ? invoke<ModsResult>("server_sync_mods", { id }) : Promise.resolve<ModsResult>({ copied: 12, skipped: ["sodium-0.6.jar"] }),

  properties: (id: string) =>
    isTauri ? invoke<[string, string][]>("server_properties", { id }) : Promise.resolve(structuredClone(previewServer(id).props)),

  setProperties: async (id: string, values: Record<string, string>) => {
    if (isTauri) return invoke<void>("server_set_properties", { id, values });
    const s = previewServer(id);
    for (const [k, v] of Object.entries(values)) {
      const row = s.props.find(([key]) => key === k);
      if (row) row[1] = v;
      else s.props.push([k, v]);
    }
  },

  addToInstance: (id: string, instanceId: string, address: string | null) =>
    isTauri ? invoke<boolean>("server_add_to_instance", { id, instanceId, address }) : Promise.resolve(true),

  stats: (id: string): Promise<ServerStats | null> =>
    isTauri
      ? invoke<ServerStats | null>("server_stats", { id })
      : Promise.resolve({ cpu: 20 + Math.random() * 30, memoryMb: 1800 + Math.random() * 400, players: 1, uptimeSecs: 754 }),

  setConfig: async (
    id: string,
    update: { memoryMb: number; jvmArgs: string; share: boolean; autoRestart?: boolean; autostart?: boolean; restartHours?: number; backupOnStart?: boolean },
  ) => {
    if (isTauri) return invoke<ServerConfig>("server_set_config", { id, update });
    const s = previewServer(id);
    s.info.config = { ...s.info.config!, ...update };
    return s.info.config;
  },

  remove: async (id: string, onProgress?: (p: { done: number; total: number }) => void) => {
    if (isTauri) {
      const progress = new Channel<{ done: number; total: number }>();
      if (onProgress) progress.onmessage = onProgress;
      return invoke<void>("server_delete", { id, progress });
    }
    for (let i = 0; i <= 10; i++) {
      onProgress?.({ done: i * 120, total: 1200 });
      await wait(80);
    }
    delete preview[id];
    previewEmit(id);
  },

  start: async (id: string) => {
    if (isTauri) return invoke<void>("server_start", { id });
    const s = previewServer(id);
    s.info.status = "starting";
    s.logs = ["[12:00:00] [Server thread/INFO]: Starting minecraft server version 1.21.1", "[12:00:00] [Server thread/INFO]: Preparing level \"world\""];
    previewEmit(id);
    await wait(900);
    s.info.status = "running";
    s.info.players = ["Steve"];
    s.logs.push("[12:00:03] [Server thread/INFO]: Done (3.104s)! For help, type \"help\"", "[12:00:07] [Server thread/INFO]: Steve joined the game");
    if (s.info.config?.tunnel) s.info.tunnel = "shard-steve.joinmc.link";
    if (s.info.config?.share) {
      s.info.share = { port: 25565, upnp: true, upnpError: null, public: "93.184.216.34:25565", cgnat: false, ipv6: "[2a00:1450:4010::1]:25565" };
    }
    previewEmit(id);
  },

  command: async (id: string, command: string) => {
    if (isTauri) return invoke<void>("server_command", { id, command });
    previewServer(id).logs.push(`> ${command}`);
    previewEmit(id);
  },

  stop: async (id: string, force = false) => {
    if (isTauri) return invoke<void>("server_stop", { id, force });
    const s = previewServer(id);
    s.info.status = "stopped";
    s.info.players = [];
    s.info.share = null;
    s.info.tunnel = null;
    s.logs.push("[12:05:00] [Server thread/INFO]: Stopping server");
    previewEmit(id);
  },

  setTunnel: async (id: string, on: boolean) => {
    if (isTauri) return invoke<ServerConfig>("server_set_tunnel", { id, on });
    const s = previewServer(id);
    s.info.config = { ...s.info.config!, tunnel: on };
    s.info.tunnel = on && s.info.status !== "stopped" ? "shard-steve.joinmc.link" : null;
    previewEmit(id);
    return s.info.config;
  },

  reshare: (id: string) => (isTauri ? invoke<Share>("server_reshare", { id }) : Promise.resolve(previewServer(id).info.share!)),
};

// ---------- туннель playit.gg ----------

export interface TunnelStatus {
  /** Агент привязан к аккаунту playit.gg */
  linked: boolean;
  running: boolean;
}

let previewLinked = false;

export const tunnelApi = {
  status: () => (isTauri ? invoke<TunnelStatus>("tunnel_status") : Promise.resolve({ linked: previewLinked, running: false })),
  linkStart: () =>
    isTauri ? invoke<{ code: string; url: string }>("tunnel_link_start") : Promise.resolve({ code: "0a1b2c3d4e", url: "https://playit.gg/claim/0a1b2c3d4e" }),
  /** Ждёт, пока пользователь подтвердит привязку в браузере */
  linkFinish: async (code: string) => {
    if (isTauri) return invoke<void>("tunnel_link_finish", { code });
    await wait(1500);
    previewLinked = true;
  },
  unlink: async () => {
    if (isTauri) return invoke<void>("tunnel_unlink");
    previewLinked = false;
  },
};

// ---------- магазин сервера: плагины, моды и ресурспак с Modrinth ----------

/** addon — плагины или моды, смотря по ядру */
export type StoreKind = "addon" | "resourcepack";

export interface StoreItem {
  kind: StoreKind;
  file: string;
  projectId: string | null;
  title: string;
  versionNumber: string | null;
  iconUrl: string | null;
}

const previewStore: Record<string, StoreItem[]> = {};

export interface FileEntry {
  name: string;
  dir: boolean;
  size: number;
  modified: number;
}

const previewFiles: Record<string, string> = {
  "server.properties": "motd=Shard preview\nmax-players=20\n",
  "ops.json": "[]\n",
  "config/paper-global.yml": "chunk-loading:\n  autoconfig-send-distance: true\n",
};

export const filesApi = {
  list: async (id: string, path: string): Promise<FileEntry[]> => {
    if (isTauri) return invoke<FileEntry[]>("server_files", { id, path });
    const prefix = path ? `${path}/` : "";
    const names = new Map<string, FileEntry>();
    for (const [file, text] of Object.entries(previewFiles)) {
      if (!file.startsWith(prefix)) continue;
      const [name, ...rest] = file.slice(prefix.length).split("/");
      names.set(name, { name, dir: rest.length > 0, size: rest.length ? 0 : text.length, modified: Date.now() / 1000 - 600 });
    }
    return [...names.values()].sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name));
  },
  read: async (id: string, path: string) => (isTauri ? invoke<string>("server_file_read", { id, path }) : (previewFiles[path] ?? "")),
  write: async (id: string, path: string, content: string) => {
    if (isTauri) return invoke<void>("server_file_write", { id, path, content });
    previewFiles[path] = content;
  },
  remove: async (id: string, path: string) => {
    if (isTauri) return invoke<void>("server_file_delete", { id, path });
    for (const key of Object.keys(previewFiles)) if (key === path || key.startsWith(`${path}/`)) delete previewFiles[key];
  },
};

export type AccessList = "ops" | "whitelist" | "banned";
export interface AccessPlayer {
  name: string;
  uuid: string;
  reason: string | null;
}
export interface Access {
  online: string[];
  ops: AccessPlayer[];
  whitelist: AccessPlayer[];
  banned: AccessPlayer[];
  whitelistOn: boolean;
  onlineMode: boolean;
}

const previewAccess: Access = { online: ["Steve"], ops: [], whitelist: [], banned: [], whitelistOn: false, onlineMode: true };

export const playersApi = {
  access: (id: string) => (isTauri ? invoke<Access>("server_access", { id }) : Promise.resolve(structuredClone(previewAccess))),
  change: async (id: string, list: AccessList, name: string, add: boolean, reason?: string) => {
    if (isTauri) return invoke<void>("server_access_change", { id, list, name, add, reason: reason ?? null });
    const items = previewAccess[list];
    previewAccess[list] = add ? [...items.filter((p) => p.name !== name), { name, uuid: name, reason: reason ?? null }] : items.filter((p) => p.name !== name);
  },
  kick: async (id: string, name: string, reason?: string) => {
    if (isTauri) return invoke<void>("server_kick", { id, name, reason: reason ?? null });
    previewAccess.online = previewAccess.online.filter((n) => n !== name);
  },
};

export const storeApi = {
  search: async (id: string, kind: StoreKind, query: string, sort: string, offset: number, categories: string[], version: string | null): Promise<SearchResult> => {
    const anyVersion = version === "*";
    const gameVersion = version && version !== "*" ? version : null;
    if (isTauri) return invoke<SearchResult>("server_store_search", { id, kind, query, sort, offset, categories, anyVersion, gameVersion });
    const info = previewServer(id).info.config;
    const plugin = info?.core === "paper" || info?.core === "purpur";
    const projectType = kind === "resourcepack" ? "resourcepack" : plugin ? "plugin" : "mod";
    return contentApi.search({ query, kind: projectType === "plugin" ? "mod" : (projectType as "mod" | "resourcepack"), instanceId: null, sort: sort as SearchSort, offset, source: "modrinth", categories, gameVersion, anyVersion });
  },
  list: (id: string) =>
    isTauri
      ? invoke<{ folder: string | null; items: StoreItem[] }>("server_store_list", { id })
      : Promise.resolve({ folder: "plugins", items: [...(previewStore[id] ?? [])] }),
  install: async (id: string, kind: StoreKind, projectId: string, anyVersion: boolean) => {
    if (isTauri) return invoke<string[]>("server_store_install", { id, kind, projectId, anyVersion });
    await wait(600);
    const items = (previewStore[id] ??= []).filter((i) => kind === "addon" || i.kind !== "resourcepack");
    items.push({ kind, file: kind === "resourcepack" ? "@resource-pack" : `${projectId}.jar`, projectId, title: projectId, versionNumber: "1.0.0", iconUrl: null });
    previewStore[id] = items;
    return [projectId];
  },
  remove: async (id: string, file: string) => {
    if (isTauri) return invoke<void>("server_store_remove", { id, file });
    previewStore[id] = (previewStore[id] ?? []).filter((i) => i.file !== file);
  },
};

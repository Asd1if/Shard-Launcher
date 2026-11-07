import { Channel, invoke } from "@tauri-apps/api/core";
import type { ByteProgress } from "@/components/ui/progress-bar";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Instance, InstanceSettings, ModLoader, VersionType } from "@/features/instances/types";

export const isTauri = "__TAURI_INTERNALS__" in window;

export interface Account {
  id: string;
  kind: "offline" | "microsoft";
  name: string;
  uuid: string;
  /** Скин офлайн-аккаунта из библиотеки (его показывает CustomSkinLoader) */
  skin?: string | null;
}

export interface Accounts {
  selected: string | null;
  accounts: Account[];
}

export interface NewInstance {
  name: string;
  mcVersion: string;
  versionType: VersionType;
  loader: ModLoader | null;
  /** null — рекомендованная, выбирается при первом запуске */
  loaderVersion: string | null;
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
  recommended: boolean;
}

export interface DeviceCode {
  userCode: string;
  verificationUri: string;
  expiresIn: number;
}

export interface InstanceUpdate {
  name: string;
  memoryMb: number;
  settings: InstanceSettings;
  loaderVersion?: string | null;
}

export interface JavaInfo {
  path: string;
  version: string;
  major: number;
  arch: string;
  vendor: string;
  managed: boolean;
}

export type CrashKind =
  | "missing_dependency"
  | "incompatible"
  | "duplicate"
  | "mixin"
  | "memory"
  | "java"
  | "graphics"
  | "native"
  | "mod_crash"
  | "unknown";

export interface CrashReport {
  kind: CrashKind;
  mods: string[];
  details: string[];
  file: string | null;
  javaMajor: number | null;
}

export interface World {
  folder: string;
  lastPlayed: string | null;
  size: number;
  icon: string | null;
  backups: number;
}

export interface Backup {
  file: string;
  world: string;
  createdAt: string;
  size: number;
}

export interface Screenshot {
  file: string;
  path: string;
  takenAt: string;
  size: number;
}

export type OnLaunch = "keep" | "minimize" | "close";

export interface Settings {
  defaultMemoryMb: number | null;
  defaultJavaPath: string | null;
  defaultJvmArgs: string;
  onLaunch: OnLaunch;
  discordDisabled: boolean;
  hideNews?: boolean;
  /** Свой ключ CurseForge API */
  curseforgeKey: string | null;
  /** Мастер первого запуска пройден */
  onboarded: boolean;
  termsVersion: number;
  /** Копировать миры после выхода из игры */
  autoBackup: boolean;
  /** Сколько копий мира хранить; 0 — 5 */
  backupKeep: number;
}

export interface DataDir {
  path: string;
  portable: boolean;
  isDefault: boolean;
}

export interface AddFilesResult {
  added: [string, "mod" | "resourcepack" | "shader"][];
  packs: string[];
  skipped: string[];
}

export type LaunchStage = "version" | "libraries" | "assets" | "java" | "loader";

export interface BackendEvents {
  "launch-progress": { instanceId: string; stage: LaunchStage; done: number; total: number };
  "game-started": { instanceId: string };
  "game-log": { instanceId: string; line: string; stderr: boolean };
  "game-exited": { instanceId: string; code: number | null; crash: CrashReport | null };
}

// В обычном браузере (превью интерфейса) Rust-бэкенда нет — держим данные в памяти.
const preview = {
  instances: [] as Instance[],
  accounts: { selected: null, accounts: [] } as Accounts,
  settings: {
    defaultMemoryMb: null,
    defaultJavaPath: null,
    defaultJvmArgs: "",
    onLaunch: "keep",
    discordDisabled: false,
    curseforgeKey: null,
    // В превью мастер первого запуска открывается по ?onboarding
    onboarded: !location.search.includes("onboarding"),
    termsVersion: 0,
    autoBackup: false,
    backupKeep: 5,
  } as Settings,
};

const previewUpdate = (id: string, fn: (i: Instance) => Instance) => {
  preview.instances = preview.instances.map((i) => (i.id === id ? fn(i) : i));
  return preview.instances.find((i) => i.id === id)!;
};

function offlinePreviewAccount(name: string): Account {
  return { id: `offline-${name.toLowerCase()}`, kind: "offline", name, uuid: "" };
}

/** Канал прогресса для Rust; без обработчика — пустой. */
export function channel(onProgress?: (p: ByteProgress) => void) {
  const ch = new Channel<ByteProgress>();
  if (onProgress) ch.onmessage = onProgress;
  return ch;
}

/** Превью без Rust: полоска пробегает за секунду, чтобы было видно, как она выглядит. */
async function fakeProgress(onProgress?: (p: ByteProgress) => void) {
  const total = 734_003_200;
  for (let step = 0; step <= 10; step++) {
    onProgress?.({ done: (total * step) / 10, total });
    await new Promise((r) => setTimeout(r, 150));
  }
}

export const backend = {
  instances: () => (isTauri ? invoke<Instance[]>("instances_list") : Promise.resolve(preview.instances)),

  createInstance: async (data: NewInstance) => {
    if (isTauri) return invoke<Instance>("instance_create", { new: data });
    const instance: Instance = {
      ...data,
      loaderVersion: data.loaderVersion ?? undefined,
      id: crypto.randomUUID(),
      modCount: 0,
      createdAt: new Date().toISOString(),
    };
    preview.instances = [instance, ...preview.instances];
    return instance;
  },

  deleteInstance: async (id: string, onProgress?: (p: ByteProgress) => void) => {
    if (isTauri) return invoke<void>("instance_delete", { id, progress: channel(onProgress) });
    await fakeProgress(onProgress);
    preview.instances = preview.instances.filter((i) => i.id !== id);
  },

  updateInstance: async (id: string, update: InstanceUpdate) => {
    if (isTauri) return invoke<Instance>("instance_update", { id, update });
    preview.instances = preview.instances.map((i) =>
      i.id === id ? { ...i, ...update, loaderVersion: update.loaderVersion ?? i.loaderVersion } : i,
    );
    return preview.instances.find((i) => i.id === id)!;
  },

  setInstanceIcon: async (id: string, icon: string | null) => {
    if (isTauri) return invoke<Instance>("instance_set_icon", { id, icon });
    preview.instances = preview.instances.map((i) => (i.id === id ? { ...i, icon: icon ?? undefined } : i));
    return preview.instances.find((i) => i.id === id)!;
  },

  setInstanceMeta: async (id: string, group: string | null, favorite: boolean) => {
    if (isTauri) return invoke<Instance>("instance_set_meta", { id, group, favorite });
    return previewUpdate(id, (i) => ({ ...i, group: group?.trim() || undefined, favorite }));
  },

  cloneInstance: async (id: string, name: string, onProgress?: (p: ByteProgress) => void) => {
    if (isTauri) return invoke<Instance>("instance_clone", { id, name, progress: channel(onProgress) });
    await fakeProgress(onProgress);
    const source = preview.instances.find((i) => i.id === id)!;
    const copy = { ...source, id: crypto.randomUUID(), name, createdAt: new Date().toISOString(), lastPlayed: undefined, favorite: false };
    preview.instances = [copy, ...preview.instances];
    return copy;
  },

  originalIcon: (id: string) =>
    isTauri ? invoke<Instance>("instance_original_icon", { id }) : Promise.reject(new Error("pack_no_icon")),

  addFiles: (id: string, paths: string[]) =>
    isTauri
      ? invoke<AddFilesResult>("content_add_files", { id, paths })
      : Promise.resolve<AddFilesResult>({ added: [], packs: [], skipped: paths }),

  worlds: (id: string) => (isTauri ? invoke<World[]>("worlds_list", { id }) : Promise.resolve<World[]>([])),

  backups: (id: string) => (isTauri ? invoke<Backup[]>("world_backups", { id }) : Promise.resolve<Backup[]>([])),

  backupWorld: (id: string, world: string, onProgress?: (p: ByteProgress) => void) =>
    invoke<Backup>("world_backup", { id, world, progress: channel(onProgress) }),

  restoreWorld: (id: string, file: string) => (isTauri ? invoke<void>("world_restore", { id, file }) : Promise.resolve()),

  deleteBackup: (id: string, file: string) => (isTauri ? invoke<void>("world_backup_delete", { id, file }) : Promise.resolve()),

  screenshots: (id: string) =>
    isTauri ? invoke<Screenshot[]>("screenshots_list", { id }) : Promise.resolve<Screenshot[]>([]),

  /** relative — от папки игры; `@/` в начале — от папки сборки. reveal — показать в проводнике */
  openPath: (id: string, relative: string, reveal = false) =>
    isTauri ? invoke<void>("instance_open_path", { id, relative, reveal }) : Promise.resolve(),

  openCrashReport: (path: string) => invoke<void>("open_crash_report", { path }),

  settings: () => (isTauri ? invoke<Settings>("settings_get") : Promise.resolve(preview.settings)),

  setSettings: async (settings: Settings) => {
    if (isTauri) return invoke<Settings>("settings_set", { settings });
    preview.settings = settings;
    return settings;
  },

  dataDir: () =>
    isTauri
      ? invoke<DataDir>("settings_data_dir")
      : Promise.resolve<DataDir>({ path: "C:/Users/you/AppData/Roaming/app.shard.launcher", portable: false, isDefault: true }),

  /** null — вернуть стандартную папку. Лаунчер перезапустится */
  checkCurseforgeKey: (key: string) =>
    isTauri ? invoke<boolean>("curseforge_check_key", { key }) : Promise.resolve(key.trim().length >= 20),

  moveData: (path: string | null, onProgress?: (p: ByteProgress) => void) =>
    invoke<void>("settings_move_data", { path, progress: channel(onProgress) }),

  loaderVersions: (loader: ModLoader, mcVersion: string) =>
    isTauri
      ? invoke<LoaderVersion[]>("loader_versions", { loader, mcVersion })
      : Promise.resolve<LoaderVersion[]>([
          { version: "1.2.0", stable: true, recommended: true },
          { version: "1.3.0-beta", stable: false, recommended: false },
          { version: "1.1.4", stable: true, recommended: false },
        ]),

  javaDetect: () => (isTauri ? invoke<JavaInfo[]>("java_detect") : Promise.resolve([] as JavaInfo[])),

  javaProbe: (path: string) =>
    isTauri ? invoke<JavaInfo>("java_probe", { path }) : Promise.reject(new Error("java_not_runnable")),

  systemInfo: () =>
    isTauri ? invoke<{ totalMemoryMb: number }>("system_info") : Promise.resolve({ totalMemoryMb: 16384 }),

  openInstanceDir: (id: string) => (isTauri ? invoke<void>("instance_open_dir", { id }) : Promise.resolve()),

  accounts: () => (isTauri ? invoke<Accounts>("accounts_list") : Promise.resolve(preview.accounts)),

  setAccountSkin: async (id: string, skin: string | null) => {
    if (isTauri) return invoke<Accounts>("account_set_skin", { id, skin });
    preview.accounts = { ...preview.accounts, accounts: preview.accounts.accounts.map((a) => (a.id === id ? { ...a, skin } : a)) };
    return preview.accounts;
  },

  addOfflineAccount: async (name: string) => {
    if (isTauri) return invoke<Accounts>("account_add_offline", { name });
    const account = offlinePreviewAccount(name);
    preview.accounts = {
      selected: account.id,
      accounts: [...preview.accounts.accounts.filter((a) => a.id !== account.id), account],
    };
    return preview.accounts;
  },

  selectAccount: async (id: string) => {
    if (isTauri) return invoke<Accounts>("account_select", { id });
    preview.accounts = { ...preview.accounts, selected: id };
    return preview.accounts;
  },

  removeAccount: async (id: string) => {
    if (isTauri) return invoke<Accounts>("account_remove", { id });
    const accounts = preview.accounts.accounts.filter((a) => a.id !== id);
    preview.accounts = { selected: accounts[0]?.id ?? null, accounts };
    return preview.accounts;
  },

  msaStart: () =>
    isTauri ? invoke<DeviceCode>("msa_start") : Promise.reject(new Error("auth:no_client_id")),

  msaFinish: () => (isTauri ? invoke<Accounts>("msa_finish") : Promise.reject(new Error("auth:cancelled"))),

  msaCancel: () => (isTauri ? invoke<void>("msa_cancel") : Promise.resolve()),


  launch: (id: string, join = false) =>
    isTauri ? invoke<void>("launch_instance", { id, join }) : Promise.reject(new Error("preview mode")),

  stop: (id: string) => (isTauri ? invoke<void>("stop_instance", { id }) : Promise.resolve()),

  running: () => (isTauri ? invoke<string[]>("running_instances") : Promise.resolve([])),

  on<K extends keyof BackendEvents>(event: K, handler: (payload: BackendEvents[K]) => void): Promise<UnlistenFn> {
    if (!isTauri) return Promise.resolve(() => {});
    return listen<BackendEvents[K]>(event, (e) => handler(e.payload));
  },
};

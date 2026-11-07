/** Загрузчик модов — необязательная надстройка над ядром (ванильным Minecraft). */
export type ModLoader = "fabric" | "quilt" | "forge" | "neoforge";

export const modLoaders: ModLoader[] = ["fabric", "quilt", "forge", "neoforge"];

/** Тип версии из манифеста Mojang. */
export type VersionType = "release" | "snapshot" | "old_beta" | "old_alpha";

export interface Instance {
  id: string;
  name: string;
  /** Ядро всегда ванильное: версия Minecraft от Mojang */
  mcVersion: string;
  versionType: VersionType;
  /** null — чистая ванилла без загрузчика */
  loader: ModLoader | null;
  loaderVersion?: string;
  modCount: number;
  /** ISO-дата последнего запуска */
  lastPlayed?: string;
  createdAt: string;
  /** Максимум памяти для игры, МБ */
  memoryMb?: number;
  settings?: InstanceSettings;
  /** `preset:<имя>` из встроенного набора или картинка data URL; нет — узор из названия */
  icon?: string;
  /** Папка в списке сборок */
  group?: string;
  favorite?: boolean;
  /** Модпак, из которого поставлена сборка */
  pack?: PackOrigin;
}

export interface PackOrigin {
  source: "modrinth" | "curseforge";
  projectId: string;
  versionId?: string;
  versionName?: string;
  iconUrl?: string;
}

/** Тонкие настройки запуска сборки. Пустое значение — поведение по умолчанию. */
export interface InstanceSettings {
  javaPath: string | null;
  minMemoryMb: number | null;
  jvmArgs: string;
  gameArgs: string;
  width: number | null;
  height: number | null;
  fullscreen: boolean;
  env: string;
  preLaunch: string;
  postExit: string;
  /** Адрес для кнопки «Зайти на сервер» */
  server: string;
}

export const defaultSettings: InstanceSettings = {
  javaPath: null,
  minMemoryMb: null,
  jvmArgs: "",
  gameArgs: "",
  width: null,
  height: null,
  fullscreen: false,
  env: "",
  preLaunch: "",
  postExit: "",
  server: "",
};

/** Какие загрузчики поддерживают версии этого типа. */
export function loadersFor(type: VersionType): ModLoader[] {
  switch (type) {
    case "release":
      return modLoaders;
    case "snapshot":
      // Fabric и Quilt выпускают сборки под снапшоты, Forge и NeoForge — нет
      return ["fabric", "quilt"];
    default:
      return [];
  }
}

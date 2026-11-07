import { Channel, invoke } from "@tauri-apps/api/core";
import { backend, isTauri } from "@/lib/backend";
import type { Instance } from "@/features/instances/types";
import type { ManualDownload, SearchResult } from "@/features/content/api";
import type { ProjectQuery } from "@/components/project-search";

export type PackFormat = "modrinth" | "curseforge";
export const packFormats: PackFormat[] = ["modrinth", "curseforge"];
export const packExtension: Record<PackFormat, string> = { modrinth: "mrpack", curseforge: "zip" };

export type PackProgress =
  | { stage: "fetch"; done: number; total: number }
  | { stage: "files"; done: number; total: number }
  | { stage: "overrides" }
  | { stage: "identify" }
  | { stage: "write"; done: number; total: number };

export interface ImportResult {
  instance: Instance;
  format: PackFormat;
  manual: ManualDownload[];
}

export interface ExportEntry {
  name: string;
  isDir: boolean;
  size: number;
  suggested: boolean;
}

export interface ExportArgs {
  format: PackFormat;
  path: string;
  name: string;
  version: string;
  author: string;
  summary: string;
  include: string[];
}

export interface ExportResult {
  linked: number;
  bundled: number;
  size: number;
}

export interface ImportSource {
  path?: string;
  url?: string;
  /** Проект-модпак CurseForge: ставится последний файл */
  curseforgeId?: number;
  /** Выбранная версия: id версии Modrinth или id файла CurseForge; нет — последняя */
  versionId?: string;
  /** Название для окна прогресса */
  name?: string;
}

export interface PackVersion {
  id: string;
  name: string;
  gameVersions: string[];
  loaders: string[];
  date: string;
  kind: "release" | "beta" | "alpha";
  /** CurseForge: автор запретил стороннюю загрузку */
  manual: boolean;
}

// В превью — правдоподобный список, чтобы было видно фильтры
function previewVersions(): PackVersion[] {
  const games = ["1.21.1", "1.21.1", "1.20.1", "1.20.1", "1.19.2"];
  return games.map((g, i) => ({
    id: `preview-${i}`,
    name: `${5 - i}.${i % 2 ? 1 : 0}.0`,
    gameVersions: [g],
    loaders: [i % 3 === 2 ? "forge" : "fabric"],
    date: new Date(Date.now() - 86_400_000 * (i * 30 + 3)).toISOString(),
    kind: i === 1 ? "beta" : "release",
    manual: false,
  }));
}

function channel(onProgress: (p: PackProgress) => void) {
  const ch = new Channel<PackProgress>();
  ch.onmessage = onProgress;
  return ch;
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

// ---------- превью интерфейса без Rust ----------

async function previewImport(source: ImportSource, onProgress: (p: PackProgress) => void): Promise<ImportResult> {
  if (source.url || source.curseforgeId) {
    for (let done = 0; done <= 8; done += 2) {
      onProgress({ stage: "fetch", done: done * 1_000_000, total: 8_000_000 });
      await wait(150);
    }
  }
  for (let done = 0; done <= 24; done += 4) {
    onProgress({ stage: "files", done, total: 24 });
    await wait(150);
  }
  onProgress({ stage: "overrides" });
  await wait(400);
  const name = source.name ?? (source.path ?? source.url ?? "Modpack").split(/[\\/]/).pop()!.replace(/\.(mrpack|zip)$/i, "");
  const instance = await backend.createInstance({
    name: name || "Modpack",
    mcVersion: "1.21.1",
    versionType: "release",
    loader: "fabric",
    loaderVersion: "0.16.10",
  });
  return { instance: { ...instance, modCount: 24 }, format: "modrinth", manual: [] };
}

const previewEntries: ExportEntry[] = [
  { name: "config", isDir: true, size: 1_480_000, suggested: true },
  { name: "mods", isDir: true, size: 38_200_000, suggested: true },
  { name: "resourcepacks", isDir: true, size: 5_100_000, suggested: true },
  { name: "saves", isDir: true, size: 220_000_000, suggested: false },
  { name: "screenshots", isDir: true, size: 12_000_000, suggested: false },
  { name: "logs", isDir: true, size: 640_000, suggested: false },
  { name: "options.txt", isDir: false, size: 5_200, suggested: true },
  { name: "servers.dat", isDir: false, size: 300, suggested: false },
];

async function previewSearch(q: ProjectQuery): Promise<SearchResult> {
  if (q.source === "curseforge") return { hits: [], totalHits: 0 };
  const params = new URLSearchParams({
    query: q.query,
    facets: JSON.stringify([
      ["project_type:modpack"],
      ...q.categories.map((c) => [`categories:${c}`]),
      ...(q.version && q.version !== "*" ? [[`versions:${q.version}`]] : []),
      ...(q.loader ? [[`categories:${q.loader}`]] : []),
    ]),
    index: q.sort,
    offset: String(q.offset),
    limit: "20",
  });
  const data = await (await fetch(`https://api.modrinth.com/v2/search?${params}`)).json();
  return {
    totalHits: data.total_hits,
    hits: data.hits.map((h: Record<string, unknown>) => ({
      source: "modrinth",
      pageUrl: `https://modrinth.com/modpack/${h.slug}`,
      projectId: h.project_id,
      slug: h.slug,
      title: h.title,
      description: h.description,
      author: h.author,
      iconUrl: h.icon_url ?? null,
      downloads: h.downloads,
      follows: h.follows,
      displayCategories: h.display_categories ?? [],
      dateModified: h.date_modified,
    })),
  };
}

export const modpackApi = {
  search: (q: ProjectQuery) =>
    isTauri
      ? invoke<SearchResult>("modpack_search", {
          args: {
            query: q.query,
            sort: q.sort,
            offset: q.offset,
            source: q.source,
            categories: q.categories,
            gameVersion: q.version && q.version !== "*" ? q.version : null,
            loaders: q.loader ? [q.loader] : [],
          },
        })
      : previewSearch(q),

  import: (source: ImportSource, onProgress: (p: PackProgress) => void) =>
    isTauri
      ? invoke<ImportResult>("modpack_import", {
          args: { path: source.path, url: source.url, curseforgeId: source.curseforgeId, versionId: source.versionId },
          progress: channel(onProgress),
        })
      : previewImport(source, onProgress),

  versions: (source: "modrinth" | "curseforge", projectId: string) =>
    isTauri ? invoke<PackVersion[]>("modpack_versions", { source, projectId }) : Promise.resolve(previewVersions()),

  /** Обновить или откатить модпак сборки до выбранной версии */
  changeVersion: async (id: string, versionId: string, onProgress: (p: PackProgress) => void): Promise<ImportResult> => {
    if (isTauri) return invoke<ImportResult>("modpack_change_version", { id, versionId, progress: channel(onProgress) });
    for (let done = 0; done <= 24; done += 6) {
      onProgress({ stage: "files", done, total: 24 });
      await wait(150);
    }
    const instance = (await backend.instances()).find((i) => i.id === id)!;
    return { instance, format: "modrinth", manual: [] };
  },

  entries: (id: string) =>
    isTauri ? invoke<ExportEntry[]>("modpack_export_entries", { id }) : Promise.resolve(previewEntries),

  export: async (id: string, args: ExportArgs, onProgress: (p: PackProgress) => void): Promise<ExportResult> => {
    if (isTauri) return invoke<ExportResult>("modpack_export", { id, args, progress: channel(onProgress) });
    onProgress({ stage: "identify" });
    await wait(700);
    for (let done = 0; done <= 40; done += 8) {
      onProgress({ stage: "write", done, total: 40 });
      await wait(120);
    }
    return { linked: 48, bundled: 40, size: 1_900_000 };
  },
};

/** Окно выбора архива сборки; null — пользователь передумал. */
export async function pickPackFile(title: string): Promise<string | null> {
  if (!isTauri) return "Preview Pack.mrpack";
  const { open } = await import("@tauri-apps/plugin-dialog");
  const path = await open({ title, multiple: false, filters: [{ name: "Modpack", extensions: ["mrpack", "zip"] }] });
  return typeof path === "string" ? path : null;
}

export async function pickSavePath(title: string, name: string, format: PackFormat): Promise<string | null> {
  const ext = packExtension[format];
  if (!isTauri) return `${name}.${ext}`;
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({ title, defaultPath: `${name}.${ext}`, filters: [{ name: format === "modrinth" ? "Modrinth" : "CurseForge", extensions: [ext] }] });
}

export const isPackFile = (path: string) => /\.(mrpack|zip)$/i.test(path);

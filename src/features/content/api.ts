import { invoke } from "@tauri-apps/api/core";
import { backend, isTauri } from "@/lib/backend";

export type ContentKind = "mod" | "resourcepack" | "shader";
export const contentKinds: ContentKind[] = ["mod", "resourcepack", "shader"];

export type ContentSource = "modrinth" | "curseforge";
export const contentSources: ContentSource[] = ["modrinth", "curseforge"];

export type SearchSort = "relevance" | "downloads" | "updated" | "newest";

export interface SearchHit {
  source: ContentSource;
  /** Страница проекта на сайте источника */
  pageUrl: string | null;
  projectId: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  iconUrl: string | null;
  downloads: number;
  follows: number;
  displayCategories: string[];
  dateModified: string;
  /** Версии игры, для которых у проекта есть файлы */
  gameVersions?: string[];
}

export interface SearchResult {
  hits: SearchHit[];
  totalHits: number;
  /** Найдено, но только для других версий игры */
  other?: SearchHit[];
}

/** Самая новая версия игры из списка, без снапшотов: «1.20.1». */
export function newestVersion(versions: string[] | undefined) {
  const release = (versions ?? []).filter((v) => /^\d+(\.\d+)+$/.test(v));
  const parts = (v: string) => v.split(".").map(Number);
  return release.sort((a, b) => {
    const x = parts(a);
    const y = parts(b);
    for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (y[i] ?? 0) - (x[i] ?? 0);
    return 0;
  })[0];
}

export interface ContentMeta {
  source: ContentSource;
  projectId: string;
  versionId: string;
  title: string;
  versionNumber: string;
  iconUrl: string | null;
}

export interface InstalledContent {
  kind: ContentKind;
  /** Имя файла на диске, с `.disabled` у отключённых */
  file: string;
  enabled: boolean;
  size: number;
  sha1: string;
  meta: ContentMeta | null;
}

export interface Installed {
  title: string;
  versionNumber: string;
  dependency: boolean;
}

/** Файл CurseForge, который автор запретил качать из сторонних программ. */
export interface ManualDownload {
  kind: ContentKind;
  fileName: string;
  url: string;
  sha1: string | null;
  size: number;
  meta: ContentMeta;
}

export interface InstallResult {
  installed: Installed[];
  manual: ManualDownload[];
}

export interface VersionInfo {
  id: string;
  name: string;
  gameVersions: string[];
  loaders: string[];
  date: string;
  channel: "release" | "beta" | "alpha";
}

export interface ProjectDetails {
  /** Markdown у Modrinth, HTML у CurseForge */
  body: string;
  html: boolean;
  gallery: { url: string; full: string; title: string | null }[];
  links: { kind: "source" | "issues" | "wiki" | "discord" | "donate"; url: string }[];
  license: string | null;
  versions: VersionInfo[];
  dependencies: { projectId: string; title: string; iconUrl: string | null; required: boolean }[];
}

/** Превью в браузере: та же страница проекта прямо с Modrinth. */
async function previewProject(projectId: string): Promise<ProjectDetails> {
  const get = (path: string) => fetch(`https://api.modrinth.com/v2/project/${projectId}${path}`).then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))));
  const [p, versions] = await Promise.all([get(""), get("/version")]);
  const links: ProjectDetails["links"] = [];
  if (p.source_url) links.push({ kind: "source", url: p.source_url });
  if (p.issues_url) links.push({ kind: "issues", url: p.issues_url });
  if (p.wiki_url) links.push({ kind: "wiki", url: p.wiki_url });
  if (p.discord_url) links.push({ kind: "discord", url: p.discord_url });
  return {
    body: p.body ?? "",
    html: false,
    gallery: (p.gallery ?? []).map((g: { url: string; raw_url?: string; title?: string }) => ({ url: g.url, full: g.raw_url ?? g.url, title: g.title || null })),
    links,
    license: p.license?.name || p.license?.id || null,
    versions: (versions as { id: string; name: string; version_number: string; game_versions: string[]; loaders: string[]; date_published: string; version_type: VersionInfo["channel"] }[]).map((v) => ({
      id: v.id,
      name: v.name || v.version_number,
      gameVersions: v.game_versions,
      loaders: v.loaders,
      date: v.date_published,
      channel: v.version_type,
    })),
    dependencies: [],
  };
}

export interface UpdateInfo {
  source: ContentSource;
  projectId: string;
  kind: ContentKind;
  file: string;
  versionId: string;
  versionNumber: string;
}

export interface SearchArgs {
  query: string;
  kind: ContentKind;
  instanceId: string | null;
  sort: SearchSort;
  offset: number;
  source: ContentSource;
  /** Не фильтровать по версии игры — например, старый ресурспак на новую сборку */
  anyVersion?: boolean;
  categories?: string[];
  /** Своя версия игры вместо версии сборки */
  gameVersion?: string | null;
  loaders?: string[];
  environment?: string | null;
}

export interface Category {
  id: string;
  name: string;
  group: string | null;
}

/** Ключ проекта без пересечений между источниками — как в бэкенде. */
export const projectKey = (source: ContentSource, id: string) => (source === "curseforge" ? `cf:${id}` : id);

// ---------- превью интерфейса без Rust ----------

async function previewModrinth(args: SearchArgs): Promise<SearchResult> {
  const result = await previewModrinthOnce(args);
  const instance = args.instanceId ? (await backend.instances()).find((i) => i.id === args.instanceId) : undefined;
  if (instance && !args.anyVersion && args.offset === 0 && args.query.trim()) {
    const all = await previewModrinthOnce({ ...args, anyVersion: true });
    result.other = all.hits
      .filter((h) => h.gameVersions?.length && !h.gameVersions.includes(instance.mcVersion))
      .filter((h) => !result.hits.some((r) => r.projectId === h.projectId))
      .slice(0, 5);
  }
  return result;
}

async function previewModrinthOnce(args: SearchArgs): Promise<SearchResult> {
  const instance = args.instanceId ? (await backend.instances()).find((i) => i.id === args.instanceId) : undefined;
  const facets = [[`project_type:${args.kind}`]];
  if (args.gameVersion) facets.push([`versions:${args.gameVersion}`]);
  else if (instance && !args.anyVersion) facets.push([`versions:${instance.mcVersion}`]);
  if (args.loaders?.length) facets.push(args.loaders.map((l) => `categories:${l}`));
  for (const c of args.categories ?? []) facets.push([`categories:${c}`]);
  if (instance?.loader && args.kind === "mod") {
    facets.push(instance.loader === "quilt" ? ["categories:quilt", "categories:fabric"] : [`categories:${instance.loader}`]);
  }
  const params = new URLSearchParams({
    query: args.query,
    facets: JSON.stringify(facets),
    index: args.sort,
    offset: String(args.offset),
    limit: "20",
  });
  const res = await fetch(`https://api.modrinth.com/v2/search?${params}`);
  const data = await res.json();
  return {
    totalHits: data.total_hits,
    hits: data.hits.map((h: Record<string, unknown>) => ({
      source: "modrinth",
      pageUrl: `https://modrinth.com/${args.kind}/${h.slug}`,
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
      gameVersions: h.versions,
    })),
  };
}

// CurseForge без ключа не отвечает — в превью показываем два выдуманных проекта,
// второй из них «запрещает» стороннюю загрузку, чтобы был виден диалог ручной загрузки.
const previewCurseforge: SearchHit[] = [
  {
    source: "curseforge",
    pageUrl: "https://www.curseforge.com/minecraft/mc-mods/example-mod",
    projectId: "100001",
    slug: "example-mod",
    title: "Example Mod",
    description: "Preview data: CurseForge needs an API key, so this list is a stand-in.",
    author: "preview",
    iconUrl: null,
    downloads: 1_250_000,
    follows: 320,
    displayCategories: [],
    dateModified: new Date(Date.now() - 86_400_000 * 9).toISOString(),
  },
  {
    source: "curseforge",
    pageUrl: "https://www.curseforge.com/minecraft/mc-mods/manual-only-mod",
    projectId: "100002",
    slug: "manual-only-mod",
    title: "Manual Only Mod",
    description: "Preview data: its author disabled third-party downloads.",
    author: "preview",
    iconUrl: null,
    downloads: 48_000,
    follows: 12,
    displayCategories: [],
    dateModified: new Date(Date.now() - 86_400_000 * 40).toISOString(),
  },
];

const previewInstalled = new Map<string, InstalledContent[]>();
const previewPending = new Set<string>();

function previewEntry(kind: ContentKind, hit: SearchHit | undefined, projectId: string, source: ContentSource): InstalledContent {
  return {
    kind,
    file: `${hit?.slug ?? projectId}${kind === "mod" ? ".jar" : ".zip"}`,
    enabled: true,
    size: 0,
    sha1: `${source}:${projectId}`,
    meta: { source, projectId, versionId: "preview", title: hit?.title ?? projectId, versionNumber: "1.0.0", iconUrl: hit?.iconUrl ?? null },
  };
}

export const contentApi = {
  project: (source: ContentSource, projectId: string) =>
    isTauri ? invoke<ProjectDetails>("content_project", { source, projectId }) : previewProject(projectId),
  installVersion: (id: string, kind: ContentKind, source: ContentSource, projectId: string, versionId: string, hit?: SearchHit) => {
    if (isTauri) return invoke<InstallResult>("content_install_version", { id, kind, source, projectId, versionId });
    return contentApi.install(id, projectId, kind, source, hit);
  },
  sources: () =>
    isTauri
      ? invoke<{ curseforge: boolean; builtIn?: boolean }>("content_sources")
      : Promise.resolve({ curseforge: true, builtIn: false }),

  categories: async (source: ContentSource, projectType: string): Promise<Category[]> => {
    if (isTauri) return invoke<Category[]>("content_categories", { source, projectType });
    if (source === "curseforge") return [{ id: "1", name: "Adventure", group: null }, { id: "2", name: "Magic", group: null }];
    const tags: { name: string; project_type: string; header: string }[] = await (await fetch("https://api.modrinth.com/v2/tag/category")).json();
    const wanted = projectType === "plugin" ? "mod" : projectType;
    return tags.filter((t) => t.project_type === wanted).map((t) => ({ id: t.name, name: t.name, group: t.header }));
  },

  search: (args: SearchArgs) => {
    if (isTauri) return invoke<SearchResult>("content_search", { args });
    if (args.source === "curseforge") {
      const q = args.query.trim().toLowerCase();
      const hits = previewCurseforge.filter((h) => !q || h.title.toLowerCase().includes(q));
      return Promise.resolve({ hits: args.offset ? [] : hits, totalHits: hits.length });
    }
    return previewModrinth(args);
  },

  /** quick — мгновенно, без сети: новые файлы без названий, их опознаёт следующий обычный вызов */
  list: (id: string, quick = false) =>
    isTauri ? invoke<InstalledContent[]>("content_list", { id, quick }) : Promise.resolve(previewInstalled.get(id) ?? []),

  install: async (
    id: string,
    projectId: string,
    kind: ContentKind,
    source: ContentSource,
    hit?: SearchHit,
    anyVersion = false,
    gameVersion: string | null = null,
  ) => {
    if (isTauri) return invoke<InstallResult>("content_install", { id, projectId, kind, source, anyVersion, gameVersion });
    const entry = previewEntry(kind, hit, projectId, source);
    if (projectId === "100002") {
      previewPending.add(entry.file);
      return {
        installed: [],
        manual: [{ kind, fileName: entry.file, url: hit?.pageUrl ?? "", sha1: null, size: 0, meta: entry.meta! }],
      } satisfies InstallResult;
    }
    previewInstalled.set(id, [...(previewInstalled.get(id) ?? []), entry]);
    return { installed: [{ title: entry.meta!.title, versionNumber: "1.0.0", dependency: false }], manual: [] } satisfies InstallResult;
  },

  /** true — файл нашёлся в «Загрузках» и перенесён в сборку */
  manualTake: async (id: string, item: ManualDownload) => {
    if (isTauri) return invoke<boolean>("content_manual_take", { id, item });
    // В превью «скачивание» завершается через несколько секунд после открытия диалога
    await new Promise((r) => setTimeout(r, 300));
    if (!previewPending.has(item.fileName) || Math.random() < 0.6) return false;
    previewPending.delete(item.fileName);
    previewInstalled.set(id, [
      ...(previewInstalled.get(id) ?? []),
      { kind: item.kind, file: item.fileName, enabled: true, size: 0, sha1: item.fileName, meta: item.meta },
    ]);
    return true;
  },

  toggle: async (id: string, kind: ContentKind, file: string, enabled: boolean) => {
    if (isTauri) return invoke<string>("content_toggle", { id, kind, file, enabled });
    const base = file.replace(/\.disabled$/, "");
    const next = enabled ? base : `${base}.disabled`;
    previewInstalled.set(
      id,
      (previewInstalled.get(id) ?? []).map((c) => (c.file === file ? { ...c, file: next, enabled } : c)),
    );
    return next;
  },

  remove: async (id: string, kind: ContentKind, file: string) => {
    if (isTauri) return invoke<void>("content_remove", { id, kind, file });
    previewInstalled.set(id, (previewInstalled.get(id) ?? []).filter((c) => c.file !== file));
  },

  updates: (id: string) =>
    isTauri ? invoke<UpdateInfo[]>("content_updates", { id }) : Promise.resolve<UpdateInfo[]>([]),

  update: (id: string, u: UpdateInfo) =>
    isTauri
      ? invoke<void>("content_update", {
          id,
          kind: u.kind,
          file: u.file,
          versionId: u.versionId,
          source: u.source,
          projectId: u.projectId,
        })
      : Promise.resolve(),
};

import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, ChevronLeft, ChevronRight, Download, Heart, Loader2, Package, RotateCw, Search, X } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { fetchVersions } from "@/features/versions/api";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/page";
import { ProjectIcon } from "@/components/project-icon";
import { ProjectDetailsDialog, type DetailsOptions } from "@/components/project-details";
import {
  contentApi,
  type Category,
  contentSources,
  projectKey,
  type ContentSource,
  type SearchHit,
  type SearchResult,
  newestVersion,
  type SearchSort,
} from "@/features/content/api";
import { errorText } from "@/lib/errors";
import { timeAgo } from "@/lib/time";
import { cn } from "@/lib/utils";

const SOURCE_KEY = "shard.contentSource";
const sorts: SearchSort[] = ["relevance", "downloads", "updated", "newest"];
const PAGE = 20;

/** Релизы Minecraft для фильтра версии — один раз на запуск */
let releases: Promise<string[]> | null = null;
function loadReleases() {
  releases ??= fetchVersions()
    .then((m) => m.versions.filter((v) => v.type === "release").map((v) => v.id))
    .catch(() => {
      releases = null;
      return [];
    });
  return releases;
}

/** Категории не меняются — грузим один раз на источник и тип */
const categoryCache = new Map<string, Promise<Category[]>>();
function loadCategories(source: ContentSource, projectType: string) {
  const key = `${source}/${projectType}`;
  let list = categoryCache.get(key);
  if (!list) {
    list = contentApi.categories(source, projectType);
    list.catch(() => categoryCache.delete(key));
    categoryCache.set(key, list);
  }
  return list;
}

function readSource(): ContentSource {
  try {
    return localStorage.getItem(SOURCE_KEY) === "curseforge" ? "curseforge" : "modrinth";
  } catch {
    return "modrinth";
  }
}

function saveSource(source: ContentSource) {
  try {
    localStorage.setItem(SOURCE_KEY, source);
  } catch {
    // без хранилища просто не запоминаем выбор
  }
}

export interface ProjectQuery {
  query: string;
  sort: SearchSort;
  offset: number;
  source: ContentSource;
  /** slug категорий Modrinth или id CurseForge */
  categories: string[];
  /** null — версия по умолчанию (сборки или сервера), "*" — любая, иначе конкретная */
  version: string | null;
  /** Загрузчик: fabric, forge… или iris, optifine у шейдеров */
  loader: string | null;
  /** client, server или both */
  environment: string | null;
}

/** Какие фильтры есть у этой выдачи. */
export interface SearchFilters {
  /** Версия по умолчанию — подпись пункта «Версия сборки»; нет — по умолчанию любая */
  baseVersion?: string;
  /** Загрузчики на выбор */
  loaders?: string[];
  /** Фильтр «клиент / сервер» (только Modrinth) */
  environment?: boolean;
  /** Установлен ли проект — для «Скрыть установленное» */
  isInstalled?: (hit: SearchHit) => boolean;
}

const loaderNames: Record<string, string> = {
  fabric: "Fabric",
  forge: "Forge",
  neoforge: "NeoForge",
  quilt: "Quilt",
  iris: "Iris",
  optifine: "OptiFine",
  canvas: "Canvas",
  vanilla: "Vanilla (core shaders)",
};

/**
 * Поиск проектов на Modrinth и CurseForge: переключатель источника, строка поиска, сортировка,
 * список с подгрузкой. Что делать с найденным, решает `action`.
 */
export function ProjectSearch({
  search,
  resetKey,
  placeholder,
  toolbar,
  notice,
  action,
  projectType,
  sources = contentSources,
  filters = {},
  details,
}: {
  search: (q: ProjectQuery) => Promise<SearchResult>;
  /** Смена значения сбрасывает выдачу (например, другой тип контента) */
  resetKey: string;
  placeholder: (source: string) => string;
  /** Слева от переключателя источника */
  toolbar?: ReactNode;
  /** Под строкой поиска */
  notice?: ReactNode;
  /** other — проект есть только для других версий игры; query — текущие фильтры */
  action: (hit: SearchHit, other: boolean, query: ProjectQuery) => ReactNode;
  /** Тип проектов для фильтра категорий: mod, resourcepack, shader, modpack, plugin */
  projectType: string;
  /** Какие источники показывать */
  sources?: ContentSource[];
  filters?: SearchFilters;
  /** Страница проекта: версия и загрузчик сборки, установка конкретной версии */
  details?: DetailsOptions;
}) {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [sort, setSort] = useState<SearchSort>("relevance");
  const [source, setSourceState] = useState<ContentSource>(readSource);
  // null — ещё не знаем, встроен ли ключ CurseForge в эту сборку
  const [curseforge, setCurseforge] = useState<boolean | null>(null);
  const activeSource: ContentSource =
    !sources.includes(source) || (source === "curseforge" && curseforge === false) ? sources[0] : source;
  const [page, setPage] = useState(0);
  const [selected, setSelected] = useState<string[]>([]);
  const [categories, setCategories] = useState<Category[] | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const [versions, setVersions] = useState<string[]>([]);
  const [loader, setLoader] = useState<string | null>(null);
  const [environment, setEnvironment] = useState<string | null>(null);
  const [hideInstalled, setHideInstalled] = useState(false);
  const envAvailable = !!filters.environment && activeSource === "modrinth";
  const loaderAvailable = !!filters.loaders?.length && !(activeSource === "curseforge" && projectType === "shader");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [other, setOther] = useState<SearchHit[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);
  const top = useRef<HTMLDivElement>(null);

  const setSource = (next: ContentSource) => {
    saveSource(next);
    setSourceState(next);
  };

  useEffect(() => {
    contentApi
      .sources()
      .then((s) => setCurseforge(s.curseforge))
      .catch(() => setCurseforge(false));
  }, []);

  useEffect(() => {
    void loadReleases().then(setVersions);
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query), 300);
    return () => clearTimeout(timer);
  }, [query]);

  // Категории у источников и типов разные — выбор сбрасываем
  useEffect(() => {
    let alive = true;
    setSelected([]);
    setCategories(null);
    void loadCategories(activeSource, projectType).then(
      (list) => alive && setCategories(list),
      () => alive && setCategories([]),
    );
    return () => {
      alive = false;
    };
  }, [activeSource, projectType]);

  const currentQuery = (nextPage = page): ProjectQuery => ({
    query: debounced,
    sort,
    offset: nextPage * PAGE,
    source: activeSource,
    categories: selected,
    version,
    loader: loaderAvailable ? loader : null,
    environment: envAvailable ? environment : null,
  });

  const load = async (nextPage: number) => {
    const id = ++request.current;
    setLoading(true);
    setError(null);
    try {
      const result = await search(currentQuery(nextPage));
      // Ответ на устаревший запрос (пользователь уже ввёл другое) отбрасываем
      if (id !== request.current) return;
      setHits(result.hits);
      setOther(nextPage === 0 ? (result.other ?? []) : []);
      setTotal(result.totalHits);
      setPage(nextPage);
    } catch (e) {
      if (id === request.current) setError(errorText(e));
    } finally {
      if (id === request.current) setLoading(false);
    }
  };

  useEffect(() => {
    // Пока не ясно, доступен ли CurseForge, не ищем — иначе мелькнёт выдача не того источника
    if (source === "curseforge" && curseforge === null) return;
    void load(0);
  }, [debounced, sort, resetKey, activeSource, curseforge === null, selected.join(), version, loader, environment]);

  const go = (p: number) => {
    void load(p);
    top.current?.scrollIntoView({ block: "start", behavior: "smooth" });
  };
  const toggle = (id: string) => setSelected((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));
  const query_ = currentQuery();
  const shown = hideInstalled && filters.isInstalled ? hits.filter((h) => !filters.isInstalled!(h)) : hits;
  const versionLabel =
    version === null ? (filters.baseVersion ? t("content.filter.versionBase", { version: filters.baseVersion }) : t("content.filter.versionAny")) : version === "*" ? t("content.filter.versionAny") : version;
  const nameOf = (c: Category) => (activeSource === "modrinth" ? categoryName(t, c.id) : c.name);
  // Выбранное — чипами под фильтрами, каждое снимается крестиком
  const chips: { key: string; label: string; clear: () => void }[] = [
    ...selected.map((id) => {
      const c = categories?.find((x) => x.id === id);
      return { key: `c:${id}`, label: c ? nameOf(c) : id, clear: () => toggle(id) };
    }),
    ...(version !== null ? [{ key: "v", label: versionLabel, clear: () => setVersion(null) }] : []),
    ...(loaderAvailable && loader ? [{ key: "l", label: loaderNames[loader] ?? loader, clear: () => setLoader(null) }] : []),
    ...(envAvailable && environment ? [{ key: "e", label: t(`content.filter.env.${environment}`), clear: () => setEnvironment(null) }] : []),
  ];
  const clearAll = () => {
    setSelected([]);
    setVersion(null);
    setLoader(null);
    setEnvironment(null);
  };
  const pages = Math.ceil(Math.min(total, 10_000) / PAGE);

  const number = new Intl.NumberFormat(i18n.language, { notation: "compact", maximumFractionDigits: 1 });
  // Открытая страница проекта; other — проект только для других версий игры
  const [opened, setOpened] = useState<{ hit: SearchHit; other: boolean } | null>(null);
  const openDependency = (projectId: string, title: string, iconUrl: string | null) => {
    const source = opened?.hit.source ?? activeSource;
    setOpened({
      other: false,
      hit: {
        source,
        projectId,
        title,
        iconUrl,
        pageUrl: source === "modrinth" ? `https://modrinth.com/project/${projectId}` : null,
        slug: projectId,
        description: "",
        author: "",
        downloads: 0,
        follows: 0,
        displayCategories: [],
        dateModified: "",
      },
    });
  };

  return (
    <div ref={top} className="flex scroll-mt-4 flex-col gap-3">
      <ProjectDetailsDialog
        hit={opened?.hit ?? null}
        onClose={() => setOpened(null)}
        action={opened && action(opened.hit, opened.other, query_)}
        options={details}
        onOpenDependency={openDependency}
      />
      <div className="flex flex-wrap items-center gap-3">
        {toolbar}
        {sources.length > 1 && (
        <div role="tablist" aria-label={t("content.source")} className="flex rounded-md bg-surface-2 p-0.5">
          {sources.map((key) => {
            const unavailable = key === "curseforge" && curseforge === false;
            return (
              <button
                key={key}
                role="tab"
                type="button"
                aria-selected={activeSource === key}
                disabled={unavailable}
                title={unavailable ? t("content.curseforgeUnavailable") : undefined}
                onClick={() => setSource(key)}
                className={cn(
                  "h-7 rounded px-2.5 text-xs text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:text-ink-muted",
                  activeSource === key && "bg-surface-0 text-foreground",
                )}
              >
                {t(`content.sources.${key}`)}
              </button>
            );
          })}
        </div>
        )}
      </div>

      <div className="flex gap-2">
        <label className="flex h-11 flex-1 items-center gap-3 rounded-lg border border-surface-3 bg-surface-1 px-4 focus-within:border-ink-muted">
          <Search className="size-4 text-ink-muted" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={placeholder(t(`content.sources.${activeSource}`))}
            aria-label={t("content.searchLabel")}
            className="h-full flex-1 bg-transparent outline-none placeholder:text-ink-muted"
          />
          {loading && <Loader2 className="size-4 animate-spin text-ink-muted" />}
        </label>
        <span className="relative">
          <select
            value={sort}
            onChange={(e) => setSort(e.target.value as SearchSort)}
            aria-label={t("content.sortLabel")}
            className="h-11 appearance-none rounded-lg border border-surface-3 bg-surface-1 pr-9 pl-3 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {sorts.map((s) => (
              <option key={s} value={s}>
                {t(`content.sort.${s}`)}
              </option>
            ))}
          </select>
          <ChevronDown className="pointer-events-none absolute top-1/2 right-3 size-3.5 -translate-y-1/2 text-ink-muted" />
        </span>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <FilterMenu label={t("content.filter.categories")} count={selected.length} disabled={categories?.length === 0}>
          {categories === null ? (
            <div className="grid place-items-center p-4">
              <Loader2 className="size-4 animate-spin text-ink-muted" />
            </div>
          ) : (
            Object.entries(
              categories.reduce<Record<string, Category[]>>((groups, c) => {
                (groups[c.group ?? ""] ??= []).push(c);
                return groups;
              }, {}),
            ).map(([group, list], i) => (
              <div key={group}>
                {i > 0 && <DropdownMenuSeparator />}
                {group && <DropdownMenuLabel className="text-[11px] tracking-wide text-ink-muted uppercase">{t(`content.categoryGroups.${group}`, { defaultValue: group })}</DropdownMenuLabel>}
                {list
                  .toSorted((x, y) => nameOf(x).localeCompare(nameOf(y), i18n.language))
                  .map((c) => (
                    <DropdownMenuCheckboxItem key={c.id} checked={selected.includes(c.id)} onSelect={(e) => e.preventDefault()} onCheckedChange={() => toggle(c.id)}>
                      {nameOf(c)}
                    </DropdownMenuCheckboxItem>
                  ))}
              </div>
            ))
          )}
        </FilterMenu>

        <FilterMenu label={t("content.filter.version")} value={version !== null ? versionLabel : undefined}>
          <DropdownMenuRadioGroup value={version ?? ""} onValueChange={(v) => setVersion(v === "" ? null : v)}>
            <DropdownMenuRadioItem value="">
              {filters.baseVersion ? t("content.filter.versionBase", { version: filters.baseVersion }) : t("content.filter.versionAny")}
            </DropdownMenuRadioItem>
            {filters.baseVersion && <DropdownMenuRadioItem value="*">{t("content.filter.versionAny")}</DropdownMenuRadioItem>}
            <DropdownMenuSeparator />
            {versions.map((v) => (
              <DropdownMenuRadioItem key={v} value={v}>
                {v}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </FilterMenu>

        {loaderAvailable && (
          <FilterMenu label={t("content.filter.loader")} value={loader ? (loaderNames[loader] ?? loader) : undefined}>
            <DropdownMenuRadioGroup value={loader ?? ""} onValueChange={(v) => setLoader(v || null)}>
              <DropdownMenuRadioItem value="">{t("content.filter.loaderAny")}</DropdownMenuRadioItem>
              {filters.loaders!.map((l) => (
                <DropdownMenuRadioItem key={l} value={l}>
                  {loaderNames[l] ?? l}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </FilterMenu>
        )}

        {envAvailable && (
          <FilterMenu label={t("content.filter.environment")} value={environment ? t(`content.filter.env.${environment}`) : undefined}>
            <DropdownMenuRadioGroup value={environment ?? ""} onValueChange={(v) => setEnvironment(v || null)}>
              <DropdownMenuRadioItem value="">{t("content.filter.envAny")}</DropdownMenuRadioItem>
              {["client", "server", "both"].map((e) => (
                <DropdownMenuRadioItem key={e} value={e}>
                  {t(`content.filter.env.${e}`)}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </FilterMenu>
        )}

        {filters.isInstalled && (
          <button
            type="button"
            aria-pressed={hideInstalled}
            onClick={() => setHideInstalled((h) => !h)}
            className={cn(
              "h-9 rounded-md border px-3 text-[13px] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
              hideInstalled ? "border-foreground bg-foreground text-background" : "border-surface-3 text-ink-muted hover:text-foreground",
            )}
          >
            {t("content.filter.hideInstalled")}
          </button>
        )}
      </div>

      {chips.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          {chips.map((c) => (
            <span key={c.key} className="flex items-center gap-1 rounded-md bg-surface-2 py-0.5 pr-1 pl-2.5 text-xs text-foreground">
              {c.label}
              <button type="button" onClick={c.clear} aria-label={t("content.filter.remove", { name: c.label })} className="grid size-5 place-items-center rounded text-ink-muted outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">
                <X className="size-3" />
              </button>
            </span>
          ))}
          <button type="button" onClick={clearAll} className="px-1.5 text-xs text-ink-muted underline-offset-2 outline-none hover:text-foreground hover:underline focus-visible:underline">
            {t("content.clearFilters")}
          </button>
        </div>
      )}

      {notice}

      {error ? (
        <EmptyState
          icon={<Search className="size-7" />}
          title={t("content.searchFailed")}
          text={error}
          action={
            <Button variant="secondary" onClick={() => void load(0)}>
              <RotateCw /> {t("create.retry")}
            </Button>
          }
        />
      ) : hits.length === 0 && other.length === 0 && !loading ? (
        <EmptyState icon={<Package className="size-7" />} title={t("content.nothingFound")} text={t("content.nothingFoundText")} />
      ) : (
        hits.length > 0 && (
          <ul className="flex flex-col divide-y divide-surface-3 rounded-lg border border-surface-3 bg-surface-1">
            {shown.map((hit) => (
              <Row key={projectKey(hit.source, hit.projectId)} hit={hit} number={number} action={action(hit, false, query_)} onOpen={() => setOpened({ hit, other: false })} />
            ))}
          </ul>
        )
      )}

      {!error && other.length > 0 && (
        <section className="flex flex-col gap-2">
          <h3 className="text-[13px] text-ink-muted">{t("content.otherVersions")}</h3>
          <ul className="flex flex-col divide-y divide-surface-3 rounded-lg border border-dashed border-surface-3 bg-surface-1/60">
            {other.map((hit) => (
              <Row
                key={projectKey(hit.source, hit.projectId)}
                hit={hit}
                number={number}
                note={newestVersion(hit.gameVersions) && t("content.onlyFor", { version: newestVersion(hit.gameVersions) })}
                action={action(hit, true, query_)}
                onOpen={() => setOpened({ hit, other: true })}
              />
            ))}
          </ul>
        </section>
      )}

      {pages > 1 && <Pager page={page} pages={pages} disabled={loading} onChange={go} />}
    </div>
  );
}

function Row({ hit, number, note, action, onOpen }: { hit: SearchHit; number: Intl.NumberFormat; note?: string | false; action: ReactNode; onOpen: () => void }) {
  const { t, i18n } = useTranslation();
  return (
    <li className="flex items-start gap-4 px-4 py-3.5">
      <button type="button" onClick={onOpen} tabIndex={-1} aria-hidden className="shrink-0 rounded-lg outline-none">
        <ProjectIcon url={hit.iconUrl} className="size-12" />
      </button>
      <div className="min-w-0 flex-1">
        <p className="flex items-baseline gap-2">
          <button
            type="button"
            onClick={onOpen}
            title={t("project.details")}
            className="truncate text-left text-foreground outline-none hover:underline focus-visible:underline"
          >
            {hit.title}
          </button>
          <span className="shrink-0 truncate text-xs text-ink-muted">{t("content.by", { author: hit.author })}</span>
        </p>
        <p className="mt-0.5 line-clamp-2 text-[13px] text-ink-muted">{hit.description}</p>
        <p className="mt-1.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-ink-muted tabular-nums">
          {note && <span className="text-[#e0951a]">{note}</span>}
          <span className="flex items-center gap-1" title={t("content.downloads")}>
            <Download className="size-3" /> {number.format(hit.downloads)}
          </span>
          <span className="flex items-center gap-1" title={t(hit.source === "curseforge" ? "content.likes" : "content.follows")}>
            <Heart className="size-3" /> {number.format(hit.follows)}
          </span>
          <span>{t("content.updated", { when: timeAgo(hit.dateModified, i18n.language) })}</span>
        </p>
      </div>
      {action}
    </li>
  );
}

/** Номера страниц: первая, последняя и соседние с текущей, между ними многоточие. */
function Pager({ page, pages, disabled, onChange }: { page: number; pages: number; disabled: boolean; onChange: (p: number) => void }) {
  const { t } = useTranslation();
  const shown = [...new Set([0, page - 2, page - 1, page, page + 1, page + 2, pages - 1])].filter((p) => p >= 0 && p < pages).sort((a, b) => a - b);
  const cell = "grid h-8 min-w-8 place-items-center rounded-md px-2 text-[13px] tabular-nums outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";
  return (
    <nav aria-label={t("content.pages")} className="flex flex-wrap items-center justify-center gap-1 pt-1">
      <button type="button" disabled={disabled || page === 0} onClick={() => onChange(page - 1)} aria-label={t("content.prevPage")} className={cn(cell, "text-ink-muted enabled:hover:bg-surface-2 enabled:hover:text-foreground")}>
        <ChevronLeft className="size-4" />
      </button>
      {shown.map((p, i) => (
        <span key={p} className="flex items-center gap-1">
          {i > 0 && p - shown[i - 1] > 1 && <span className="px-1 text-ink-muted">…</span>}
          <button
            type="button"
            disabled={disabled}
            aria-current={p === page ? "page" : undefined}
            onClick={() => p !== page && onChange(p)}
            className={cn(cell, p === page ? "bg-foreground text-background" : "text-ink-muted enabled:hover:bg-surface-2 enabled:hover:text-foreground")}
          >
            {p + 1}
          </button>
        </span>
      ))}
      <button type="button" disabled={disabled || page >= pages - 1} onClick={() => onChange(page + 1)} aria-label={t("content.nextPage")} className={cn(cell, "text-ink-muted enabled:hover:bg-surface-2 enabled:hover:text-foreground")}>
        <ChevronRight className="size-4" />
      </button>
    </nav>
  );
}

/** Название категории Modrinth: перевод, если есть, иначе slug по-человечески. */
function categoryName(t: (key: string, o?: Record<string, unknown>) => string, slug: string) {
  const human = slug.replace(/-/g, " ").replace(/^./, (c) => c.toUpperCase());
  return t(`categories.${slug}`, { defaultValue: human });
}

/** Кнопка фильтра с выпадающим меню; value — выбранное, count — сколько отмечено. */
function FilterMenu({ label, value, count, disabled, children }: { label: string; value?: string; count?: number; disabled?: boolean; children: ReactNode }) {
  const active = !!value || !!count;
  return (
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger
        disabled={disabled}
        className={cn(
          "flex h-9 items-center gap-1.5 rounded-md border px-3 text-[13px] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 data-[state=open]:border-ink-muted",
          active ? "border-surface-3 bg-surface-2 text-foreground" : "border-surface-3 text-ink-muted hover:text-foreground",
        )}
      >
        <span className="whitespace-nowrap">
          {label}
          {value && ":"}
        </span>
        {value && <span className="max-w-32 truncate text-foreground">{value}</span>}
        {!!count && <span className="rounded bg-brand px-1.5 text-[11px] text-brand-foreground tabular-nums">{count}</span>}
        <ChevronDown className="size-3.5 opacity-70" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="max-h-80 min-w-52 border-surface-3 bg-surface-1">
        {children}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

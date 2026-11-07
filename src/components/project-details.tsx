import { useEffect, useMemo, useState, type MouseEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { ArrowLeft, ArrowRight, Download, ExternalLink, Heart, Loader2, X } from "lucide-react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { ProjectIcon } from "@/components/project-icon";
import { contentApi, type ProjectDetails, type SearchHit, type VersionInfo } from "@/features/content/api";
import { isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { timeAgo } from "@/lib/time";
import { cn } from "@/lib/utils";

export interface DetailsOptions {
  /** Версия игры и загрузчик сборки — для фильтра «подходящие версии» */
  gameVersion?: string;
  loader?: string | null;
  /** Поставить конкретную версию; без него в списке версий нет кнопок */
  installVersion?: (hit: SearchHit, version: VersionInfo) => Promise<void>;
}

const open = (url: string) => void (isTauri ? openUrl(url) : window.open(url, "_blank", "noopener"));

/** Описание с сайта: Markdown или HTML, только безопасные теги, ссылки — во внешнем браузере. */
function Body({ details }: { details: ProjectDetails }) {
  const html = useMemo(() => {
    const raw = details.html ? details.body : (marked.parse(details.body, { async: false, gfm: true, breaks: false }) as string);
    return DOMPurify.sanitize(raw, { FORBID_TAGS: ["style", "form", "input", "button", "textarea", "select"], FORBID_ATTR: ["style", "class", "id"] });
  }, [details]);

  const onClick = (e: MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href");
    if (href && /^https?:\/\//.test(href)) open(href);
  };

  return (
    <div
      onClick={onClick}
      className="project-body text-[13.5px] leading-relaxed text-foreground/90"
      // Санитизировано DOMPurify выше
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

function Gallery({ shots }: { shots: ProjectDetails["gallery"] }) {
  const [current, setCurrent] = useState<number | null>(null);
  const shot = current !== null ? shots[current] : null;
  return (
    <>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
        {shots.map((s, i) => (
          <button
            key={s.url}
            type="button"
            onClick={() => setCurrent(i)}
            className="group overflow-hidden rounded-md border border-surface-3 bg-surface-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <img src={s.url} alt={s.title ?? ""} loading="lazy" className="aspect-video w-full object-cover transition-transform group-hover:scale-[1.03]" />
            {s.title && <span className="block truncate px-2 py-1.5 text-xs text-ink-muted">{s.title}</span>}
          </button>
        ))}
      </div>
      {shot && current !== null && (
        <div
          className="fixed inset-0 z-[60] flex items-center justify-center bg-black/85 p-6"
          onClick={() => setCurrent(null)}
          onKeyDown={(e) => {
            if (e.key === "ArrowRight") setCurrent((current + 1) % shots.length);
            if (e.key === "ArrowLeft") setCurrent((current - 1 + shots.length) % shots.length);
            if (e.key === "Escape") {
              e.stopPropagation();
              setCurrent(null);
            }
          }}
          tabIndex={-1}
          ref={(el) => el?.focus()}
        >
          <img src={shot.full} alt={shot.title ?? ""} className="max-h-full max-w-full rounded-md object-contain" onClick={(e) => e.stopPropagation()} />
          {shots.length > 1 && (
            <>
              <button type="button" aria-label="←" onClick={(e) => (e.stopPropagation(), setCurrent((current - 1 + shots.length) % shots.length))} className="absolute left-4 grid size-10 place-items-center rounded-full bg-white/10 text-white hover:bg-white/20">
                <ArrowLeft className="size-5" />
              </button>
              <button type="button" aria-label="→" onClick={(e) => (e.stopPropagation(), setCurrent((current + 1) % shots.length))} className="absolute right-4 grid size-10 place-items-center rounded-full bg-white/10 text-white hover:bg-white/20">
                <ArrowRight className="size-5" />
              </button>
            </>
          )}
          <button type="button" aria-label="✕" onClick={() => setCurrent(null)} className="absolute top-4 right-4 grid size-9 place-items-center rounded-full bg-white/10 text-white hover:bg-white/20">
            <X className="size-5" />
          </button>
        </div>
      )}
    </>
  );
}

function Versions({ hit, versions, options }: { hit: SearchHit; versions: VersionInfo[]; options: DetailsOptions }) {
  const { t, i18n } = useTranslation();
  const loaderOk = (v: VersionInfo) => !options.loader || v.loaders.length === 0 || v.loaders.includes(options.loader) || (options.loader === "quilt" && v.loaders.includes("fabric"));
  const fits = (v: VersionInfo) => (!options.gameVersion || v.gameVersions.includes(options.gameVersion)) && loaderOk(v);
  const canFilter = !!options.gameVersion && versions.some(fits);
  const [onlyFit, setOnlyFit] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const list = canFilter && onlyFit ? versions.filter(fits) : versions;

  const install = async (v: VersionInfo) => {
    if (!options.installVersion) return;
    setBusy(v.id);
    try {
      await options.installVersion(hit, v);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="flex flex-col gap-2">
      {canFilter && (
        <label className="flex cursor-pointer items-center gap-2 self-start text-xs text-ink-muted select-none hover:text-foreground">
          <input type="checkbox" checked={onlyFit} onChange={(e) => setOnlyFit(e.target.checked)} />
          {t("project.onlyFit", { version: options.gameVersion })}
        </label>
      )}
      {list.length === 0 ? (
        <p className="py-6 text-sm text-ink-muted">{t("project.noVersions")}</p>
      ) : (
        <ul className="flex flex-col divide-y divide-surface-3 rounded-lg border border-surface-3">
          {list.map((v) => {
            const games = v.gameVersions;
            return (
              <li key={v.id} className="flex items-center gap-3 px-3 py-2">
                <span
                  title={t(`project.channel.${v.channel}`)}
                  className={cn(
                    "grid size-6 shrink-0 place-items-center rounded text-[11px] font-medium uppercase",
                    v.channel === "release" ? "bg-[#1bd96a]/15 text-[#1bd96a]" : v.channel === "beta" ? "bg-[#e0951a]/15 text-[#e0951a]" : "bg-[#ff6b6b]/15 text-[#ff6b6b]",
                  )}
                >
                  {v.channel[0]}
                </span>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13px] text-foreground">{v.name}</p>
                  <p className="truncate text-[11.5px] text-ink-muted">
                    {[v.loaders.map((l) => l[0].toUpperCase() + l.slice(1)).join(", "), games.slice(0, 4).join(", ") + (games.length > 4 ? ` +${games.length - 4}` : ""), timeAgo(v.date, i18n.language)]
                      .filter(Boolean)
                      .join(" · ")}
                  </p>
                </div>
                {options.installVersion && (
                  <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void install(v)} className="shrink-0">
                    {busy === v.id ? <Loader2 className="animate-spin" /> : <Download />}
                    {t("project.installThis")}
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

type Tab = "about" | "gallery" | "versions";

/** Страница проекта поверх поиска: описание, скриншоты, версии, зависимости. */
export function ProjectDetailsDialog({
  hit,
  onClose,
  action,
  options = {},
  onOpenDependency,
}: {
  hit: SearchHit | null;
  onClose: () => void;
  /** Та же кнопка установки, что в строке поиска */
  action?: ReactNode;
  options?: DetailsOptions;
  onOpenDependency?: (projectId: string, title: string, iconUrl: string | null) => void;
}) {
  const { t, i18n } = useTranslation();
  const [details, setDetails] = useState<ProjectDetails | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("about");
  const number = useMemo(() => new Intl.NumberFormat(i18n.language, { notation: "compact", maximumFractionDigits: 1 }), [i18n.language]);

  useEffect(() => {
    setDetails(null);
    setError(null);
    setTab("about");
    if (!hit) return;
    let alive = true;
    contentApi.project(hit.source, hit.projectId).then(
      (d) => alive && setDetails(d),
      (e) => alive && setError(errorText(e)),
    );
    return () => {
      alive = false;
    };
  }, [hit]);

  const tabs: Tab[] = details?.gallery.length ? ["about", "gallery", "versions"] : ["about", "versions"];

  return (
    <Dialog open={!!hit} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="flex h-[calc(100dvh-3rem)] max-h-[860px] flex-col gap-0 p-0 sm:max-w-4xl">
        {hit && (
          <>
            <header className="flex shrink-0 items-start gap-4 border-b border-surface-3 p-5 pr-12">
              <ProjectIcon url={hit.iconUrl} className="size-16" />
              <div className="min-w-0 flex-1">
                <DialogTitle className="truncate text-lg text-foreground">{hit.title}</DialogTitle>
                <DialogDescription className="mt-0.5 line-clamp-2 text-[13px] text-ink-muted">{hit.description}</DialogDescription>
                {hit.author && <p className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-ink-muted tabular-nums">
                  <span>{t("content.by", { author: hit.author })}</span>
                  <span className="flex items-center gap-1">
                    <Download className="size-3" /> {number.format(hit.downloads)}
                  </span>
                  <span className="flex items-center gap-1">
                    <Heart className="size-3" /> {number.format(hit.follows)}
                  </span>
                  <span>{t("content.updated", { when: timeAgo(hit.dateModified, i18n.language) })}</span>
                </p>}
              </div>
              <div className="flex shrink-0 flex-col items-end gap-2">
                {action}
                {hit.pageUrl && (
                  <Button variant="ghost" size="sm" onClick={() => open(hit.pageUrl!)} className="text-ink-muted">
                    <ExternalLink /> {t("project.openSite")}
                  </Button>
                )}
              </div>
            </header>

            <div role="tablist" className="flex shrink-0 gap-1 border-b border-surface-3 px-5 py-2">
              {tabs.map((k) => (
                <button
                  key={k}
                  role="tab"
                  type="button"
                  aria-selected={tab === k}
                  onClick={() => setTab(k)}
                  className={cn(
                    "h-8 rounded-md px-3 text-[13px] text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                    tab === k && "bg-surface-2 text-foreground",
                  )}
                >
                  {t(`project.tabs.${k}`)}
                  {k === "gallery" && details && <span className="ml-1.5 text-ink-muted tabular-nums">{details.gallery.length}</span>}
                  {k === "versions" && details && <span className="ml-1.5 text-ink-muted tabular-nums">{details.versions.length}</span>}
                </button>
              ))}
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto p-5">
              {error ? (
                <p className="text-sm text-[#ff6b6b]">{error}</p>
              ) : !details ? (
                <Loader2 className="size-5 animate-spin text-ink-muted" />
              ) : tab === "about" ? (
                <div className="flex flex-col-reverse gap-6 md:flex-row">
                  <div className="min-w-0 flex-1">
                    <Body details={details} />
                  </div>
                  {(details.dependencies.length > 0 || details.links.length > 0 || details.license) && (
                    <aside className="flex shrink-0 flex-col gap-5 md:w-56">
                      {details.dependencies.length > 0 && (
                        <section className="flex flex-col gap-2">
                          <h3 className="text-xs text-ink-muted">{t("project.dependencies")}</h3>
                          <ul className="flex flex-col gap-1">
                            {details.dependencies.map((d) => (
                              <li key={d.projectId}>
                                <button
                                  type="button"
                                  disabled={!onOpenDependency}
                                  onClick={() => onOpenDependency?.(d.projectId, d.title, d.iconUrl)}
                                  className="flex w-full items-center gap-2 rounded-md p-1.5 text-left outline-none enabled:hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-ring"
                                >
                                  <ProjectIcon url={d.iconUrl} className="size-8" />
                                  <span className="min-w-0">
                                    <span className="block truncate text-[13px] text-foreground">{d.title}</span>
                                    <span className="block text-[11px] text-ink-muted">{t(d.required ? "project.required" : "project.optional")}</span>
                                  </span>
                                </button>
                              </li>
                            ))}
                          </ul>
                        </section>
                      )}
                      {details.links.length > 0 && (
                        <section className="flex flex-col gap-1">
                          <h3 className="pb-1 text-xs text-ink-muted">{t("project.links")}</h3>
                          {details.links.map((l) => (
                            <button key={l.url} type="button" onClick={() => open(l.url)} className="flex items-center gap-2 rounded-md px-1.5 py-1 text-left text-[13px] text-foreground outline-none hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-ring">
                              <ExternalLink className="size-3.5 text-ink-muted" /> {t(`project.link.${l.kind}`)}
                            </button>
                          ))}
                        </section>
                      )}
                      {details.license && (
                        <section>
                          <h3 className="pb-1 text-xs text-ink-muted">{t("project.license")}</h3>
                          <p className="text-[13px] text-foreground">{details.license}</p>
                        </section>
                      )}
                    </aside>
                  )}
                </div>
              ) : tab === "gallery" ? (
                <Gallery shots={details.gallery} />
              ) : (
                <Versions hit={hit} versions={details.versions} options={options} />
              )}
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

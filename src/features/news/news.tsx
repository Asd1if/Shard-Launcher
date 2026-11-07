import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import DOMPurify from "dompurify";
import { Loader2, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { backend, isTauri } from "@/lib/backend";
import { timeAgo } from "@/lib/time";
import { cn } from "@/lib/utils";

export interface News {
  title: string;
  version: string;
  kind: "release" | "snapshot";
  image: string | null;
  date: string;
  text: string;
  path: string;
}

const preview: News[] = [
  { title: "Minecraft 26.4 Snapshot 7", version: "26.4-snapshot-7", kind: "snapshot", image: null, date: new Date(Date.now() - 4 * 86_400_000).toISOString(), text: "A brand new music disc and a flurry of new blocks.", path: "" },
  { title: "Minecraft 26.3", version: "26.3", kind: "release", image: null, date: new Date(Date.now() - 30 * 86_400_000).toISOString(), text: "The latest game drop is out now.", path: "" },
];

// Лента меняется редко — грузим один раз за запуск
let cache: Promise<News[]> | null = null;
const loadNews = () => (cache ??= isTauri ? invoke<News[]>("mc_news").catch((e) => ((cache = null), Promise.reject(e))) : Promise.resolve(preview));

/** Новости Minecraft: последние релизы и снапшоты, полный текст и создание сборки на этой версии. */
export function MinecraftNews({ onCreate }: { onCreate: (version: string) => void }) {
  const { t, i18n } = useTranslation();
  const [news, setNews] = useState<News[] | null>(null);
  const [opened, setOpened] = useState<News | null>(null);

  useEffect(() => {
    // Новости можно выключить в настройках — тогда и не загружаем
    void backend.settings().then((settings) => {
      if (settings.hideNews) return setNews([]);
      void loadNews().then(setNews, () => setNews([]));
    });
  }, []);

  if (!news?.length) return null;
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm text-ink-muted">{t("news.title")}</h2>
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-2">
        {news.map((n) => (
          <li key={n.version}>
            <button
              type="button"
              onClick={() => setOpened(n)}
              className="flex h-full w-full gap-3 rounded-lg border border-transparent bg-surface-1/85 p-3 text-left backdrop-blur-md outline-none transition-colors hover:border-surface-3 focus-visible:ring-2 focus-visible:ring-ring"
            >
              {n.image && <img src={n.image} alt="" loading="lazy" className="size-16 shrink-0 rounded-md object-cover" />}
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-1.5">
                  <span className="truncate text-[13.5px] text-foreground">{n.title}</span>
                </span>
                <span className="mt-0.5 flex items-center gap-1.5 text-xs text-ink-muted">
                  <span className={cn("rounded px-1.5 py-px text-[11px]", n.kind === "release" ? "bg-brand/15 text-brand" : "bg-surface-2")}>
                    {t(`news.kind.${n.kind}`)}
                  </span>
                  {timeAgo(n.date, i18n.language)}
                </span>
                <span className="mt-1 line-clamp-2 text-xs text-ink-muted">{n.text}</span>
              </span>
            </button>
          </li>
        ))}
      </ul>
      {opened && <NewsDialog news={opened} onClose={() => setOpened(null)} onCreate={onCreate} />}
    </section>
  );
}

function NewsDialog({ news, onClose, onCreate }: { news: News; onClose: () => void; onCreate: (version: string) => void }) {
  const { t, i18n } = useTranslation();
  const [body, setBody] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri || !news.path) return setBody(`<p>${news.text}</p>`);
    void invoke<string>("mc_news_body", { path: news.path }).then(setBody, () => setBody(`<p>${news.text}</p>`));
  }, [news]);

  const html = useMemo(
    () => (body ? DOMPurify.sanitize(body, { FORBID_TAGS: ["style", "form", "input", "button", "textarea", "select", "iframe"], FORBID_ATTR: ["style", "class", "id"] }) : ""),
    [body],
  );

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="flex max-h-[85vh] flex-col border-surface-3 bg-surface-1 sm:max-w-[720px]">
        <DialogHeader>
          <DialogTitle className="font-display text-lg font-medium">{news.title}</DialogTitle>
          <DialogDescription>{new Date(news.date).toLocaleDateString(i18n.language, { day: "numeric", month: "long", year: "numeric" })}</DialogDescription>
        </DialogHeader>
        <Button
          className="self-start"
          onClick={() => {
            onClose();
            onCreate(news.version);
          }}
        >
          <Plus /> {t("news.create", { version: news.version })}
        </Button>
        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
          {body === null ? (
            <Loader2 className="mx-auto my-6 animate-spin text-ink-muted" />
          ) : (
            // Санитизировано DOMPurify выше
            <div className="project-body text-[13.5px] leading-relaxed text-foreground/90" dangerouslySetInnerHTML={{ __html: html }} />
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

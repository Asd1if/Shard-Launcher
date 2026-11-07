import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, Check, Download, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ProjectSearch } from "@/components/project-search";
import type { Instance } from "@/features/instances/types";
import { cn } from "@/lib/utils";
import { contentApi, contentKinds, projectKey, type ContentKind } from "./api";
import { installContent, installContentVersion, useContent } from "./store";

/** Поиск модов, ресурспаков и шейдеров под конкретную сборку — внутри её страницы. */
export function ContentBrowser({
  instance,
  kind,
  onKindChange,
  onBack,
}: {
  instance: Instance;
  kind: ContentKind;
  onKindChange: (kind: ContentKind) => void;
  onBack: () => void;
}) {
  const { t } = useTranslation();
  const content = useContent(instance.id);
  const installed = useMemo(
    () => new Set((content.items ?? []).flatMap((c) => (c.meta ? [projectKey(c.meta.source, c.meta.projectId)] : []))),
    [content.items],
  );
  // Модам и шейдерам нужен загрузчик; ресурспакам — нет
  const blocked = !instance.loader && kind !== "resourcepack";

  return (
    <div className="flex flex-col gap-3 pb-6">
      <ProjectSearch
        resetKey={`${instance.id}/${kind}`}
        projectType={kind}
        details={{
          gameVersion: instance.mcVersion,
          loader: kind === "mod" ? instance.loader : null,
          installVersion: (hit, v) => installContentVersion(instance.id, kind, hit, v.id),
        }}
        filters={{
          baseVersion: instance.mcVersion,
          loaders: kind === "shader" ? ["iris", "optifine", "canvas", "vanilla"] : undefined,
          environment: kind === "mod",
          isInstalled: (hit) => installed.has(projectKey(hit.source, hit.projectId)),
        }}
        search={(q) =>
          contentApi.search({
            query: q.query,
            sort: q.sort,
            offset: q.offset,
            source: q.source,
            categories: q.categories,
            kind,
            instanceId: instance.id,
            anyVersion: q.version === "*",
            gameVersion: q.version && q.version !== "*" ? q.version : null,
            loaders: q.loader ? [q.loader] : [],
            environment: q.environment,
          })
        }
        placeholder={(source) => t(`content.searchPlaceholder.${kind}`, { source })}
        toolbar={
          <>
            <Button variant="ghost" size="sm" onClick={onBack} className="text-ink-muted">
              <ArrowLeft /> {t("content.backToList")}
            </Button>
            <div role="tablist" className="flex gap-1">
              {contentKinds.map((key) => (
                <button
                  key={key}
                  role="tab"
                  type="button"
                  aria-selected={kind === key}
                  onClick={() => onKindChange(key)}
                  className={cn(
                    "h-8 rounded-md px-3 text-[13px] text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                    kind === key && "bg-surface-2 text-foreground",
                  )}
                >
                  {t(`content.kinds.${key}`)}
                </button>
              ))}
            </div>
            <span className="flex-1" />
          </>
        }
        notice={
          blocked && (
            <p className="rounded-lg border border-[#e0951a]/30 bg-[#e0951a]/8 px-4 py-3 text-[13px] text-[#e0951a]">
              {t(kind === "shader" ? "content.shadersNeedLoader" : "content.modsNeedLoader")}
            </p>
          )
        }
        action={(hit, other, q) => {
          const picked = q.version && q.version !== "*" && q.version !== instance.mcVersion ? q.version : null;
          const any = q.version === "*";
          const key = projectKey(hit.source, hit.projectId);
          const isInstalled = installed.has(key);
          const busy = content.busy.includes(key);
          // Мод под другую версию игры не запустится; ресурспак или шейдер обычно работает
          if ((other || picked) && kind === "mod" && !isInstalled) {
            return (
              <span className="mt-2 w-[124px] shrink-0 text-center text-xs text-ink-muted">
                {t("content.notForVersion", { version: instance.mcVersion })}
              </span>
            );
          }
          return (
            <Button
              size="sm"
              variant={isInstalled ? "ghost" : "secondary"}
              disabled={isInstalled || busy || blocked}
              onClick={() => void installContent(instance.id, kind, hit, any || other, picked)}
              className="mt-1 w-[124px] shrink-0"
            >
              {busy ? <Loader2 className="animate-spin" /> : isInstalled ? <Check /> : <Download />}
              {busy ? t("content.installing") : isInstalled ? t("content.isInstalled") : t("content.install")}
            </Button>
          );
        }}
      />
    </div>
  );
}

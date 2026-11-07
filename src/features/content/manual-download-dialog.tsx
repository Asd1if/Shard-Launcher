import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, ExternalLink, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ProjectIcon } from "@/components/project-icon";
import { isTauri } from "@/lib/backend";
import { closeManual, pollManual, useManual } from "./store";

const POLL_MS = 2000;

/** Файлы CurseForge, которые нужно скачать в браузере: Shard ждёт их в «Загрузках». */
export function ManualDownloadDialog() {
  const { t } = useTranslation();
  const state = useManual();
  const open = !!state;
  const left = state?.items.filter((i) => !i.done).length ?? 0;

  useEffect(() => {
    if (!open || left === 0) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      await pollManual();
      if (!stopped) timer = setTimeout(tick, POLL_MS);
    };
    timer = setTimeout(tick, POLL_MS);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [open, left]);

  const openPage = (url: string) => void (isTauri ? openUrl(url) : window.open(url));

  return (
    <Dialog open={open} onOpenChange={(o) => !o && closeManual()}>
      <DialogContent className="gap-5 border-surface-3 bg-surface-1 sm:max-w-[560px]">
        <DialogHeader>
          <DialogTitle className="font-display text-lg font-medium">{t("content.manual.title")}</DialogTitle>
          <DialogDescription>{t("content.manual.description")}</DialogDescription>
        </DialogHeader>

        <ul className="flex max-h-72 flex-col divide-y divide-surface-3 overflow-y-auto rounded-lg border border-surface-3 bg-surface-0">
          {state?.items.map((item) => (
            <li key={item.fileName} className="flex items-center gap-3 px-3 py-2.5">
              <ProjectIcon url={item.meta.iconUrl} className="size-9" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-[13px] text-foreground">{item.meta.title}</p>
                <p className="truncate text-xs text-ink-muted">{item.fileName}</p>
              </div>
              {item.done ? (
                <span className="flex items-center gap-1.5 text-xs text-ink-muted">
                  <Check className="size-3.5 text-[#3fb950]" /> {t("content.manual.done")}
                </span>
              ) : (
                <Button size="sm" variant="secondary" onClick={() => openPage(item.url)}>
                  <ExternalLink /> {t("content.manual.open")}
                </Button>
              )}
            </li>
          ))}
        </ul>

        <DialogFooter className="items-center sm:justify-between">
          <p className="flex items-center gap-2 text-xs text-ink-muted">
            {left > 0 ? (
              <>
                <Loader2 className="size-3.5 animate-spin" /> {t("content.manual.waiting", { count: left })}
              </>
            ) : (
              t("content.manual.allDone")
            )}
          </p>
          <Button variant={left > 0 ? "ghost" : "default"} onClick={closeManual}>
            {left > 0 ? t("content.manual.later") : t("content.manual.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

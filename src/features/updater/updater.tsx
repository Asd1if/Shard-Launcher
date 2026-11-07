import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { ArrowUpCircle, Loader2, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { showToast } from "@/components/toast";
import { ProgressBar, type ByteProgress } from "@/components/ui/progress-bar";
import { channel, isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { formatSize } from "@/lib/format";
import { createStore } from "@/lib/store";

export interface UpdateInfo {
  current: string;
  version: string;
  notes: string;
  url: string;
  size: number;
}

const store = createStore<{ info: UpdateInfo | null; checking: boolean; open: boolean }>({ info: null, checking: false, open: false });
export const useUpdate = store.use;

/** Проверка обновления. `manual` — по кнопке: тогда сообщаем и об отсутствии обновлений. */
export async function checkUpdate(manual = false) {
  if (!isTauri) return;
  store.set((s) => ({ ...s, checking: true }));
  try {
    const info = await invoke<UpdateInfo | null>("update_check");
    store.set((s) => ({ ...s, info, open: manual && !!info }));
    if (manual && !info) showToast(i18nText("updater.latest"));
  } catch (e) {
    if (manual) showToast(errorText(e));
  } finally {
    store.set((s) => ({ ...s, checking: false }));
  }
}

// Тексты для тостов вне компонентов
let i18nText: (key: string) => string = (k) => k;

/** Есть ли у этой сборки источник обновлений (репозиторий задаётся при сборке). */
let available: Promise<boolean> | null = null;
export const updatesAvailable = () => (available ??= isTauri ? invoke<boolean>("update_available").catch(() => false) : Promise.resolve(false));

/** Значок «Доступно обновление» в заголовке окна и окно обновления. Проверяет при запуске и раз в 6 часов. */
export function UpdateChip() {
  const { t } = useTranslation();
  const { info, open } = useUpdate();
  i18nText = t;

  useEffect(() => {
    let timer: number | undefined;
    void updatesAvailable().then((ok) => {
      if (!ok) return;
      void checkUpdate();
      timer = window.setInterval(() => void checkUpdate(), 6 * 3600_000);
    });
    return () => window.clearInterval(timer);
  }, []);

  if (!info) return null;
  return (
    <>
      <button
        type="button"
        onClick={() => store.set((s) => ({ ...s, open: true }))}
        className="mx-2 flex h-6 items-center gap-1.5 self-center rounded-md bg-brand/15 px-2 text-xs text-brand outline-none transition-colors hover:bg-brand/25 focus-visible:ring-2 focus-visible:ring-ring"
      >
        <ArrowUpCircle className="size-3.5" /> {t("updater.chip", { version: info.version })}
      </button>
      {open && <UpdateDialog info={info} onClose={() => store.set((s) => ({ ...s, open: false }))} />}
    </>
  );
}

function UpdateDialog({ info, onClose }: { info: UpdateInfo; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ByteProgress | null>(null);
  const notes = useMemo(
    () => (info.notes.trim() ? DOMPurify.sanitize(marked.parse(info.notes, { async: false }), { FORBID_TAGS: ["style", "form", "input", "iframe"], FORBID_ATTR: ["style"] }) : ""),
    [info.notes],
  );

  const install = async () => {
    setBusy(true);
    try {
      // Лаунчер закроется сам, новая версия запустится после установки
      await invoke("update_install", { progress: channel(setProgress) });
    } catch (e) {
      showToast(errorText(e));
      setBusy(false);
      setProgress(null);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="flex max-h-[80vh] flex-col border-surface-3 bg-surface-1 sm:max-w-[560px]">
        <DialogHeader>
          <DialogTitle className="font-display text-lg font-medium">{t("updater.title", { version: info.version })}</DialogTitle>
          <DialogDescription>
            {t("updater.text", { current: info.current })} · {formatSize(info.size, i18n.language)}
          </DialogDescription>
        </DialogHeader>
        {notes && (
          <div className="min-h-0 flex-1 overflow-y-auto pr-1">
            {/* Санитизировано DOMPurify выше */}
            <div className="project-body text-[13.5px] leading-relaxed text-foreground/90" dangerouslySetInnerHTML={{ __html: notes }} />
          </div>
        )}
        {busy && <ProgressBar progress={progress} label={progress && progress.done >= progress.total ? t("updater.starting") : t("updater.installing")} />}
        <DialogFooter>
          <Button variant="ghost" disabled={busy} onClick={onClose}>
            {t("updater.later")}
          </Button>
          <Button disabled={busy} onClick={() => void install()}>
            {busy ? <Loader2 className="animate-spin" /> : <ArrowUpCircle />} {busy ? t("updater.installing") : t("updater.install")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Кнопка «Проверить обновления» для настроек. */
export function CheckUpdatesButton() {
  const { t } = useTranslation();
  const { checking } = useUpdate();
  const [ok, setOk] = useState(false);
  useEffect(() => void updatesAvailable().then(setOk), []);
  if (!ok) return null;
  return (
    <Button size="sm" variant="ghost" disabled={checking} onClick={() => void checkUpdate(true)}>
      {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />} {t("updater.check")}
    </Button>
  );
}

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Clock } from "lucide-react";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { backend, isTauri } from "@/lib/backend";
import { createStore } from "@/lib/store";

export interface PlayTime {
  totalSecs: number;
  sessions: number;
  days: Record<string, number>;
}

const totals = createStore<Record<string, number>>({});

async function refreshTotals() {
  if (!isTauri) {
    totals.set({});
    return;
  }
  try {
    totals.set(await invoke<Record<string, number>>("playtime_all"));
  } catch {
    // нет данных — просто не показываем
  }
}

void refreshTotals();
// Сессия записывается при выходе из игры — тогда и обновляем
void backend.on("game-exited", () => void setTimeout(() => void refreshTotals(), 500));

export const usePlaytimes = totals.use;

/** «12 ч 5 мин», «40 мин», «< 1 мин». */
export function formatPlaytime(secs: number, t: (k: string, o?: Record<string, unknown>) => string) {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (h >= 100) return t("playtime.h", { h });
  if (h) return t("playtime.hm", { h, m });
  if (m) return t("playtime.m", { m });
  return t("playtime.lessMinute");
}

const DAYS = 14;

/** Кнопка с наигранным временем и окно со статистикой по дням. */
export function PlaytimeButton({ instanceId, name }: { instanceId: string; name: string }) {
  const { t, i18n } = useTranslation();
  const total = usePlaytimes()[instanceId];
  const [open, setOpen] = useState(false);
  const [data, setData] = useState<PlayTime | null>(null);

  useEffect(() => {
    if (!open) return;
    if (!isTauri) {
      setData({ totalSecs: 0, sessions: 0, days: {} });
      return;
    }
    void invoke<PlayTime>("playtime_get", { id: instanceId }).then(setData);
  }, [open, instanceId]);

  if (!total) return null;

  const days = Array.from({ length: DAYS }, (_, i) => {
    const d = new Date();
    d.setDate(d.getDate() - (DAYS - 1 - i));
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    return { key, date: d, secs: data?.days[key] ?? 0 };
  });
  const max = Math.max(1800, ...days.map((d) => d.secs));
  const week = days.slice(-7).reduce((s, d) => s + d.secs, 0);

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        title={t("playtime.title")}
        className="flex shrink-0 items-center gap-1 rounded bg-surface-2 px-1.5 py-px text-xs text-ink-muted outline-none hover:bg-surface-3 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
      >
        <Clock className="size-3" /> {formatPlaytime(total, t)}
      </button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="border-surface-3 bg-surface-1 sm:max-w-[520px]">
          <DialogHeader>
            <DialogTitle className="font-display text-lg font-medium">{t("playtime.title")}</DialogTitle>
            <DialogDescription>{name}</DialogDescription>
          </DialogHeader>
          {data && (
            <div className="flex flex-col gap-5">
              <div className="grid grid-cols-3 gap-3">
                {[
                  { label: t("playtime.total"), value: formatPlaytime(data.totalSecs, t) },
                  { label: t("playtime.week"), value: formatPlaytime(week, t) },
                  { label: t("playtime.sessions"), value: String(data.sessions) },
                ].map((c) => (
                  <div key={c.label} className="rounded-lg border border-surface-3 bg-surface-0 px-3 py-2.5">
                    <p className="text-xs text-ink-muted">{c.label}</p>
                    <p className="text-lg text-foreground tabular-nums">{c.value}</p>
                  </div>
                ))}
              </div>
              <div>
                <p className="mb-2 text-xs text-ink-muted">{t("playtime.lastDays", { count: DAYS })}</p>
                <div className="flex h-32 items-end gap-1">
                  {days.map((d) => (
                    <div key={d.key} className="group relative flex h-full flex-1 flex-col justify-end">
                      <div
                        className="w-full rounded-t-sm bg-brand/80 transition-colors group-hover:bg-brand"
                        style={{ height: d.secs ? `${Math.max(4, (d.secs / max) * 100)}%` : "2px", opacity: d.secs ? 1 : 0.25 }}
                      />
                      <span className="pointer-events-none absolute bottom-full left-1/2 mb-1 hidden -translate-x-1/2 rounded bg-surface-3 px-1.5 py-0.5 text-[11px] whitespace-nowrap text-foreground group-hover:block">
                        {d.date.toLocaleDateString(i18n.language, { day: "numeric", month: "short" })}: {d.secs ? formatPlaytime(d.secs, t) : "—"}
                      </span>
                    </div>
                  ))}
                </div>
                <div className="mt-1 flex justify-between text-[11px] text-ink-muted">
                  <span>{days[0].date.toLocaleDateString(i18n.language, { day: "numeric", month: "short" })}</span>
                  <span>{t("playtime.today")}</span>
                </div>
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}

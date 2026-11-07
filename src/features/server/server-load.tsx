import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { showToast } from "@/components/toast";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { Archive, Loader2, RotateCcw, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { formatSize } from "@/lib/format";
import { serverApi, serverBackupApi, type ServerBackup, type ServerConfig, type ServerStats } from "./api";
import { Card } from "./server-tab";

const POINTS = 90;
const EVERY_MS = 2000;

/** Линия последних значений; max — верх шкалы. */
function Spark({ values, max, className }: { values: number[]; max: number; className: string }) {
  const w = 240;
  const h = 48;
  if (values.length < 2) return <svg viewBox={`0 0 ${w} ${h}`} className="h-12 w-full" aria-hidden />;
  const step = w / (POINTS - 1);
  const offset = (POINTS - values.length) * step;
  const pts = values.map((v, i) => `${(offset + i * step).toFixed(1)},${(h - 2 - (Math.min(v, max) / max) * (h - 4)).toFixed(1)}`);
  return (
    <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" className={cn("h-12 w-full", className)} aria-hidden>
      <polygon points={`${offset.toFixed(1)},${h} ${pts.join(" ")} ${w},${h}`} className="fill-current opacity-15" />
      <polyline points={pts.join(" ")} fill="none" stroke="currentColor" strokeWidth={1.5} vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
    </svg>
  );
}

function uptime(t: (k: string, o?: Record<string, unknown>) => string, secs: number) {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h ? t("load.hm", { h, m }) : t("load.m", { m });
}

/** Нагрузка запущенного сервера: процессор, память, игроки — последние 3 минуты. */
export function ServerLoad({ id, config }: { id: string; config: ServerConfig }) {
  const { t } = useTranslation();
  const [history, setHistory] = useState<ServerStats[]>([]);

  useEffect(() => {
    setHistory([]);
    let alive = true;
    const tick = () =>
      void serverApi.stats(id).then((s) => {
        if (alive && s) setHistory((h) => [...h, s].slice(-POINTS));
      });
    tick();
    const timer = window.setInterval(tick, EVERY_MS);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [id]);

  const last = history.at(-1);
  const maxPlayers = Math.max(4, ...history.map((s) => s.players));
  const cells = [
    { key: "cpu", label: t("load.cpu"), value: last ? `${Math.round(last.cpu)}%` : "—", values: history.map((s) => s.cpu), max: 100, color: "text-[#3b82f6]" },
    {
      key: "ram",
      label: t("load.memory"),
      value: last ? `${(last.memoryMb / 1024).toFixed(1)} / ${(config.memoryMb / 1024).toFixed(1).replace(/\.0$/, "")} GB` : "—",
      values: history.map((s) => s.memoryMb),
      // Процесс Java берёт сверх -Xmx на метаданные и стеки
      max: config.memoryMb * 1.25,
      color: "text-[#a855f7]",
    },
    { key: "players", label: t("load.players"), value: last ? String(last.players) : "—", values: history.map((s) => s.players), max: maxPlayers, color: "text-[#3fb950]" },
  ];

  return (
    <Card>
      <div className="flex items-baseline justify-between gap-3">
        <h3 className="text-sm text-foreground">{t("load.title")}</h3>
        {last && <span className="text-xs text-ink-muted">{t("load.uptime", { time: uptime(t, last.uptimeSecs) })}</span>}
      </div>
      <div className="grid gap-4 sm:grid-cols-3">
        {cells.map((c) => (
          <div key={c.key} className="flex flex-col gap-1">
            <p className="flex items-baseline justify-between text-xs text-ink-muted">
              {c.label}
              <span className="text-[13px] text-foreground tabular-nums">{c.value}</span>
            </p>
            <Spark values={c.values} max={c.max} className={c.color} />
          </div>
        ))}
      </div>
    </Card>
  );
}

/** Автозапуск с лаунчером, перезапуск после падения и по расписанию. Сохраняется сразу. */
export function ServerAutomation({ id, config, onSaved }: { id: string; config: ServerConfig; onSaved: () => void }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const save = async (patch: Partial<Pick<ServerConfig, "autostart" | "autoRestart" | "restartHours" | "backupOnStart">>) => {
    setBusy(true);
    try {
      await serverApi.setConfig(id, { memoryMb: config.memoryMb, jvmArgs: config.jvmArgs, share: config.share, ...patch });
      onSaved();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const row = "flex items-center justify-between gap-3 text-xs text-ink-muted";
  return (
    <Card>
      <h3 className="text-foreground">{t("load.automation")}</h3>
      <label className={row}>
        <span>
          {t("load.autostart")}
          <span className="block text-[11px]">{t("load.autostartHint")}</span>
        </span>
        <input type="checkbox" className="size-4 accent-[var(--brand)]" checked={config.autostart} disabled={busy} onChange={(e) => void save({ autostart: e.target.checked })} />
      </label>
      <label className={row}>
        <span>
          {t("load.autoRestart")}
          <span className="block text-[11px]">{t("load.autoRestartHint")}</span>
        </span>
        <input type="checkbox" className="size-4 accent-[var(--brand)]" checked={config.autoRestart} disabled={busy} onChange={(e) => void save({ autoRestart: e.target.checked })} />
      </label>
      <label className={row}>
        <span>
          {t("load.backupOnStart")}
          <span className="block text-[11px]">{t("load.backupOnStartHint")}</span>
        </span>
        <input type="checkbox" className="size-4 accent-[var(--brand)]" checked={config.backupOnStart} disabled={busy} onChange={(e) => void save({ backupOnStart: e.target.checked })} />
      </label>
      <label className={row}>
        <span>
          {t("load.schedule")}
          <span className="block text-[11px]">{t("load.scheduleHint")}</span>
        </span>
        <select
          value={config.restartHours}
          disabled={busy}
          onChange={(e) => void save({ restartHours: Number(e.target.value) })}
          className="h-8 rounded-md border border-surface-3 bg-surface-1 px-2 text-[13px] text-foreground"
        >
          {[0, 3, 6, 12, 24].map((h) => (
            <option key={h} value={h}>
              {h ? t("load.everyHours", { count: h }) : t("load.never")}
            </option>
          ))}
        </select>
      </label>
    </Card>
  );
}

/** Копии мира сервера: сделать, восстановить, удалить. */
export function ServerBackups({ id, running }: { id: string; running: boolean }) {
  const { t, i18n } = useTranslation();
  const [list, setList] = useState<ServerBackup[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const load = () => void serverBackupApi.list(id).then(setList, () => setList([]));
  useEffect(load, [id]);

  const run = async (key: string, fn: () => Promise<unknown>, done?: string) => {
    setBusy(key);
    setConfirm(null);
    try {
      await fn();
      if (done) showToast(done);
      load();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Card>
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-foreground">{t("serverBackup.title")}</h3>
          <p className="text-xs text-ink-muted">{t("serverBackup.hint")}</p>
        </div>
        <Button size="sm" variant="secondary" disabled={busy !== null} onClick={() => void run("make", () => serverBackupApi.make(id), t("serverBackup.made"))}>
          {busy === "make" ? <Loader2 className="animate-spin" /> : <Archive />} {t("serverBackup.make")}
        </Button>
      </div>
      {list && list.length > 0 && (
        <ul className="flex max-h-72 flex-col divide-y divide-surface-3 overflow-y-auto">
          {list.map((b) => (
            <li key={b.file} className="flex items-center gap-3 py-2">
              <span className="min-w-0 flex-1 text-[13px] text-foreground">
                {new Date(b.createdAt).toLocaleString(i18n.language)}
                <span className="ml-2 text-xs text-ink-muted">{formatSize(b.size, i18n.language)}</span>
              </span>
              {confirm === b.file ? (
                <>
                  <Button size="sm" variant="secondary" onClick={() => void run(b.file, () => serverBackupApi.restore(id, b.file), t("serverBackup.restored"))}>
                    {t("worlds.restore")}
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setConfirm(null)}>
                    {t("files.cancel")}
                  </Button>
                </>
              ) : (
                <>
                  <Button size="sm" variant="ghost" disabled={busy !== null || running} title={running ? t("hosting.stopToEdit") : undefined} onClick={() => setConfirm(b.file)}>
                    {busy === b.file ? <Loader2 className="animate-spin" /> : <RotateCcw />} {t("worlds.restore")}
                  </Button>
                  <Button size="icon" variant="ghost" disabled={busy !== null} aria-label={t("files.delete")} title={t("files.delete")} onClick={() => void run(b.file, () => serverBackupApi.remove(id, b.file))}>
                    <Trash2 />
                  </Button>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

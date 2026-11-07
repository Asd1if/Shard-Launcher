import { useCallback, useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Ban, Crown, Loader2, LogOut, Plus, ShieldCheck, Trash2, UserRound } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { showToast } from "@/components/toast";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { playersApi, type Access, type AccessList, type AccessPlayer } from "./api";
import { Card } from "./server-tab";

const head = (name: string) => `https://mc-heads.net/avatar/${encodeURIComponent(name)}/32`;

function Avatar({ name }: { name: string }) {
  const [failed, setFailed] = useState(false);
  return failed ? (
    <span className="grid size-7 place-items-center rounded bg-surface-2 text-ink-muted">
      <UserRound className="size-4" />
    </span>
  ) : (
    <img src={head(name)} alt="" onError={() => setFailed(true)} className="size-7 rounded [image-rendering:pixelated]" />
  );
}

/** Игроки онлайн и списки доступа: операторы, белый список, баны. */
export function ServerPlayers({ id, running }: { id: string; running: boolean }) {
  const { t } = useTranslation();
  const [access, setAccess] = useState<Access | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const reload = useCallback(() => void playersApi.access(id).then(setAccess, (e) => showToast(errorText(e))), [id]);
  useEffect(reload, [reload]);
  // Онлайн меняется сам по себе — обновляем, пока сервер запущен
  useEffect(() => {
    if (!running) return;
    const timer = window.setInterval(reload, 4000);
    return () => window.clearInterval(timer);
  }, [running, reload]);

  const act = async (key: string, run: () => Promise<void>) => {
    setBusy(key);
    try {
      await run();
      // Запущенный сервер переписывает свои файлы не сразу
      if (running) await new Promise((r) => setTimeout(r, 700));
      reload();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(null);
    }
  };
  const change = (list: AccessList, name: string, add: boolean, reason?: string) =>
    act(`${list}/${name}/${add}`, () => playersApi.change(id, list, name, add, reason));

  if (!access) return <Loader2 className="my-6 size-5 animate-spin text-ink-muted" />;

  const isOp = (n: string) => access.ops.some((p) => p.name.toLowerCase() === n.toLowerCase());

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <h3 className="flex items-center gap-2 text-sm text-foreground">
          {t("players.online")}
          <span className="text-ink-muted tabular-nums">{access.online.length}</span>
        </h3>
        {!running ? (
          <p className="text-sm text-ink-muted">{t("players.offline")}</p>
        ) : access.online.length === 0 ? (
          <p className="text-sm text-ink-muted">{t("players.nobody")}</p>
        ) : (
          <ul className="flex flex-col divide-y divide-surface-3">
            {access.online.map((name) => (
              <li key={name} className="flex items-center gap-3 py-2">
                <Avatar name={name} />
                <span className="min-w-0 flex-1 truncate text-[13.5px] text-foreground">
                  {name}
                  {isOp(name) && <Crown className="ml-1.5 inline size-3.5 text-[#e0951a]" />}
                </span>
                <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void change("ops", name, !isOp(name))}>
                  <Crown /> {t(isOp(name) ? "players.deop" : "players.op")}
                </Button>
                <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void act(`kick/${name}`, () => playersApi.kick(id, name))}>
                  <LogOut /> {t("players.kick")}
                </Button>
                <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void change("banned", name, true)} className="text-[#ff6b6b] hover:text-[#ff6b6b]">
                  <Ban /> {t("players.ban")}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <div className="grid gap-4 lg:grid-cols-3">
        <ListCard
          icon={<Crown className="size-4 text-[#e0951a]" />}
          title={t("players.ops")}
          hint={t("players.opsHint")}
          players={access.ops}
          busy={busy}
          list="ops"
          onChange={change}
        />
        <ListCard
          icon={<ShieldCheck className="size-4 text-[#3fb950]" />}
          title={t("players.whitelist")}
          hint={access.whitelistOn ? t("players.whitelistOn") : t("players.whitelistOff")}
          players={access.whitelist}
          busy={busy}
          list="whitelist"
          onChange={change}
        />
        <ListCard
          icon={<Ban className="size-4 text-[#ff6b6b]" />}
          title={t("players.banned")}
          hint={t("players.bannedHint")}
          players={access.banned}
          busy={busy}
          list="banned"
          onChange={change}
        />
      </div>
      {!running && <p className="text-xs text-ink-muted">{t(access.onlineMode ? "players.stoppedOnline" : "players.stoppedOffline")}</p>}
    </div>
  );
}

function ListCard({
  icon,
  title,
  hint,
  players,
  busy,
  list,
  onChange,
}: {
  icon: React.ReactNode;
  title: string;
  hint: string;
  players: AccessPlayer[];
  busy: string | null;
  list: AccessList;
  onChange: (list: AccessList, name: string, add: boolean) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const valid = /^[A-Za-z0-9_]{3,16}$/.test(name.trim());
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!valid) return;
    await onChange(list, name.trim(), true);
    setName("");
  };
  return (
    <Card>
      <div>
        <h3 className="flex items-center gap-2 text-sm text-foreground">
          {icon} {title} <span className="text-ink-muted tabular-nums">{players.length}</span>
        </h3>
        <p className="mt-0.5 text-xs text-ink-muted">{hint}</p>
      </div>
      <form onSubmit={(e) => void submit(e)} className="flex gap-2">
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t("players.nick")} maxLength={16} className="h-8 text-[13px]" aria-label={t("players.nick")} />
        <Button type="submit" size="sm" variant="secondary" disabled={!valid || busy !== null} aria-label={t("players.add")}>
          {busy === `${list}/${name.trim()}/true` ? <Loader2 className="animate-spin" /> : <Plus />}
        </Button>
      </form>
      {players.length > 0 && (
        <ul className="flex max-h-72 flex-col overflow-y-auto">
          {players.map((p) => (
            <li key={p.uuid || p.name} className="flex items-center gap-2.5 py-1.5">
              <Avatar name={p.name} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13px] text-foreground">{p.name}</span>
                {p.reason && <span className="block truncate text-[11px] text-ink-muted">{p.reason}</span>}
              </span>
              <Button
                size="icon"
                variant="ghost"
                disabled={busy !== null}
                onClick={() => void onChange(list, p.name, false)}
                title={t("players.remove")}
                aria-label={t("players.remove")}
                className={cn("size-7")}
              >
                {busy === `${list}/${p.name}/false` ? <Loader2 className="animate-spin" /> : <Trash2 />}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

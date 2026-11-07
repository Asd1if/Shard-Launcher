import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useTranslation } from "react-i18next";
import { ArrowLeft, Check, Loader2, Pencil, Play, Plus, Server, Square, Users } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState, Page } from "@/components/page";
import { showToast } from "@/components/toast";
import { errorText } from "@/lib/errors";
import { serverApi, type ServerEntry } from "@/features/server/api";
import { CreateServer, ServerView, StatusDot } from "@/features/server/server-tab";
import { useServer } from "@/features/server/store";

/** Все серверы: статус, игроки, запуск и остановка. Сервер не привязан к сборке. */
export function ServersPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [list, setList] = useState<ServerEntry[] | null>(null);

  useEffect(() => {
    void serverApi.list().then(setList, (e) => {
      setList([]);
      showToast(errorText(e));
    });
  }, []);

  const create = (
    <Button onClick={() => navigate("/servers/new")}>
      <Plus /> {t("servers.create")}
    </Button>
  );

  return (
    <Page title={t("servers.title")} actions={list?.length ? create : undefined}>
      {list === null ? (
        <Loader2 className="size-5 animate-spin text-ink-muted" />
      ) : list.length === 0 ? (
        <EmptyState icon={<Server className="size-8" />} title={t("servers.emptyTitle")} text={t("servers.emptyText")} action={create} />
      ) : (
        <ul className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3">
          {list.map((s) => (
            <ServerCard key={s.id} entry={s} />
          ))}
        </ul>
      )}
    </Page>
  );
}

function ServerCard({ entry }: { entry: ServerEntry }) {
  const { t } = useTranslation();
  const { info } = useServer(entry.id);
  const [busy, setBusy] = useState(false);
  const config = info?.config ?? entry.config;
  const status = info?.status ?? "stopped";
  const stopped = status === "stopped";

  const toggle = async () => {
    setBusy(true);
    try {
      if (stopped) await serverApi.start(entry.id);
      else await serverApi.stop(entry.id);
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <li className="flex flex-col gap-3 rounded-lg border border-surface-3 bg-surface-1 p-4">
      <Link to={`/servers/${entry.id}`} className="flex min-w-0 items-center gap-3 outline-none focus-visible:underline">
        <span className="pixel-corners grid size-11 shrink-0 place-items-center bg-surface-3 text-ink-muted [--step:3px]">
          <Server className="size-5" />
        </span>
        <span className="min-w-0">
          <span className="block truncate text-foreground">{config.name || t(`hosting.core.${config.core}`)}</span>
          <span className="block truncate text-xs text-ink-muted">
            {t(`hosting.core.${config.core}`)} {config.mcVersion}
          </span>
        </span>
      </Link>
      <div className="flex items-center gap-2 text-xs text-ink-muted">
        <StatusDot status={status} />
        <span>{t(`hosting.status.${status}`)}</span>
        {status === "running" && (
          <span className="flex items-center gap-1">
            · <Users className="size-3" /> {info?.players.length ?? 0}
          </span>
        )}
        <span className="flex-1" />
        <Button size="sm" variant={stopped ? "secondary" : "ghost"} disabled={busy || status === "stopping"} onClick={() => void toggle()}>
          {busy ? <Loader2 className="animate-spin" /> : stopped ? <Play /> : <Square />}
          {stopped ? t("hosting.start") : t("hosting.stop")}
        </Button>
      </div>
      {info?.lan && !stopped && <p className="truncate font-mono text-xs text-ink-muted">{info.tunnel ?? info.share?.public ?? info.lan}</p>}
    </li>
  );
}

function Back() {
  const { t } = useTranslation();
  return (
    <Link to="/servers" className="flex w-fit items-center gap-1.5 text-[13px] text-ink-muted outline-none hover:text-foreground focus-visible:underline">
      <ArrowLeft className="size-4" /> {t("servers.title")}
    </Link>
  );
}

export function NewServerPage() {
  const navigate = useNavigate();
  return (
    <Page className="gap-4">
      <Back />
      <CreateServer onCancel={() => navigate("/servers")} onDone={(id) => navigate(`/servers/${id}`, { replace: true })} />
    </Page>
  );
}

export function ServerPage() {
  const { t } = useTranslation();
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const { info, reload } = useServer(id);
  const [editing, setEditing] = useState<string | null>(null);
  const name = info?.config?.name || (info?.config ? t(`hosting.core.${info.config.core}`) : "");

  const rename = async () => {
    if (editing === null) return;
    try {
      await serverApi.rename(id, editing);
      reload();
    } catch (e) {
      showToast(errorText(e));
    }
    setEditing(null);
  };

  if (info && !info.config) {
    return (
      <Page className="gap-4">
        <Back />
        <p className="text-ink-muted">{t("errors.serverMissing")}</p>
      </Page>
    );
  }

  return (
    <Page className="gap-4">
      <Back />
      <div className="flex min-h-9 items-center gap-2">
        {editing !== null ? (
          <form
            className="flex items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void rename();
            }}
          >
            <Input autoFocus value={editing} maxLength={64} onChange={(e) => setEditing(e.target.value)} onBlur={() => void rename()} className="h-9 w-72" aria-label={t("hosting.name")} />
            <Button type="submit" variant="ghost" size="icon" aria-label={t("hosting.save")}>
              <Check />
            </Button>
          </form>
        ) : (
          <>
            <h1 className="truncate font-display text-2xl font-medium tracking-tight">{name}</h1>
            {info?.config && (
              <Button variant="ghost" size="icon" title={t("servers.rename")} aria-label={t("servers.rename")} onClick={() => setEditing(name)}>
                <Pencil />
              </Button>
            )}
          </>
        )}
      </div>
      <ServerView id={id} onDeleted={() => navigate("/servers", { replace: true })} />
    </Page>
  );
}

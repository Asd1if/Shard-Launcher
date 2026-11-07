import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Search, Send, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { showToast } from "@/components/toast";
import { ShareLogButton } from "@/features/logs/share-log";
import { isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { serverApi, type ServerInfo } from "./api";

export interface ConsoleSource {
  info: ServerInfo | null;
  logs: string[];
  exitCode?: number | null;
  addLocal: (line: string) => void;
}

/** Команды ванили и популярных плагинов — для подсказок по Tab. */
const COMMANDS = [
  "help", "list", "say", "tell", "msg", "me", "kick", "ban", "ban-ip", "pardon", "pardon-ip", "banlist", "op", "deop",
  "whitelist add", "whitelist remove", "whitelist on", "whitelist off", "whitelist list", "whitelist reload",
  "gamemode survival", "gamemode creative", "gamemode adventure", "gamemode spectator", "defaultgamemode",
  "difficulty peaceful", "difficulty easy", "difficulty normal", "difficulty hard",
  "time set day", "time set night", "time set noon", "time set midnight", "weather clear", "weather rain", "weather thunder",
  "tp", "teleport", "give", "clear", "effect give", "effect clear", "enchant", "experience add", "kill", "spawnpoint", "setworldspawn",
  "gamerule keepInventory true", "gamerule keepInventory false", "gamerule doDaylightCycle", "gamerule doMobSpawning", "gamerule playersSleepingPercentage",
  "save-all", "save-on", "save-off", "seed", "worldborder set", "locate structure", "tick query", "stop", "reload", "plugins", "version", "tps", "mspt",
];
/** После этих команд идёт ник игрока */
const TAKES_PLAYER = /^(kick|ban|pardon|op|deop|tell|msg|tp|teleport|give|clear|kill|spawnpoint|whitelist (add|remove)|gamemode \w+|effect (give|clear)|enchant|experience add)$/;

type Level = "all" | "warn" | "error";
const levelOf = (line: string): Level | "cmd" | "info" =>
  line.startsWith("> ") ? "cmd" : /\/(ERROR|FATAL)\]|\bERROR\b|Exception/.test(line) ? "error" : /\/WARN\]/.test(line) ? "warn" : "info";

/** Подсветка найденного текста. */
function mark(text: string, query: string): ReactNode {
  if (!query) return text;
  const lower = text.toLowerCase();
  const q = query.toLowerCase();
  const out: ReactNode[] = [];
  let from = 0;
  for (let i = lower.indexOf(q); i !== -1; i = lower.indexOf(q, from)) {
    out.push(text.slice(from, i), <mark key={i} className="rounded-sm bg-[#e0951a]/35 text-inherit">{text.slice(i, i + q.length)}</mark>);
    from = i + q.length;
  }
  out.push(text.slice(from));
  return out;
}

function Line({ line, query }: { line: string; query: string }) {
  const level = levelOf(line);
  // [12:00:00 INFO]: или [12:00:00] [Server thread/INFO]: — приглушаем
  const head = /^(\[[\d:]+[^\]]*\](?: \[[^\]]+\])?:?)/.exec(line)?.[1] ?? "";
  return (
    <div
      className={cn(
        "break-all whitespace-pre-wrap",
        level === "cmd" ? "text-brand" : level === "error" ? "text-destructive" : level === "warn" ? "text-[#e0951a]" : "text-ink",
      )}
    >
      {head && <span className="opacity-50">{mark(head, query)}</span>}
      {mark(line.slice(head.length), query)}
    </div>
  );
}

/** Консоль сервера: цвета уровней, поиск и фильтр, подсказки команд и ников по Tab. */
export function ServerConsole({ id, server }: { id: string; server: ConsoleSource }) {
  const { t } = useTranslation();
  const [command, setCommand] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [cursor, setCursor] = useState(-1);
  const [query, setQuery] = useState("");
  const [level, setLevel] = useState<Level>("all");
  const [searching, setSearching] = useState(false);
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const status = server.info!.status;
  const players = server.info!.players;

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return server.logs.filter((line) => {
      const l = levelOf(line);
      if (level === "error" && l !== "error") return false;
      if (level === "warn" && l !== "warn" && l !== "error") return false;
      return !q || line.toLowerCase().includes(q);
    });
  }, [server.logs, query, level]);
  const counts = useMemo(() => {
    let warn = 0;
    let error = 0;
    for (const line of server.logs) {
      const l = levelOf(line);
      if (l === "warn") warn++;
      if (l === "error") error++;
    }
    return { warn, error };
  }, [server.logs]);

  useEffect(() => {
    const el = scroller.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [shown.length]);

  // Подсказки: сначала команда, после неё — ник
  const suggestions = useMemo(() => {
    const text = command.replace(/^\//, "");
    if (!text) return [];
    const lastSpace = text.lastIndexOf(" ");
    const before = text.slice(0, lastSpace).trim();
    const word = text.slice(lastSpace + 1).toLowerCase();
    const fromPlayers = lastSpace > 0 && TAKES_PLAYER.test(before) ? players.filter((p) => p.toLowerCase().startsWith(word)).map((p) => `${before} ${p}`) : [];
    const fromCommands = COMMANDS.filter((c) => c.startsWith(text.toLowerCase()) && c !== text.toLowerCase());
    return [...fromPlayers, ...fromCommands].slice(0, 6);
  }, [command, players]);

  const send = async () => {
    const text = command.trim();
    if (!text) return;
    try {
      await serverApi.command(id, text);
      if (isTauri) server.addLocal(`> ${text}`);
      setHistory((h) => [text, ...h.filter((x) => x !== text)].slice(0, 50));
      setCursor(-1);
      setCommand("");
    } catch (e) {
      showToast(errorText(e));
    }
  };

  const footer =
    status === "stopped" && server.exitCode !== undefined && server.exitCode !== 0 && server.exitCode !== null ? t("hosting.crashed", { code: server.exitCode }) : null;

  const levelButton = (key: Level, label: string, count?: number) => (
    <button
      type="button"
      aria-pressed={level === key}
      onClick={() => setLevel(key)}
      className={cn(
        "h-6 rounded px-2 text-[11.5px] text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
        level === key && "bg-surface-2 text-foreground",
      )}
    >
      {label}
      {!!count && <span className={cn("ml-1 tabular-nums", key === "error" ? "text-destructive" : "text-[#e0951a]")}>{count}</span>}
    </button>
  );

  return (
    <section className="flex h-[460px] flex-col overflow-hidden rounded-lg border border-surface-3 bg-surface-1">
      <div className="flex items-center gap-1 border-b border-surface-3 px-2 py-1">
        {levelButton("all", t("console.all"))}
        {levelButton("warn", t("console.warnings"), counts.warn)}
        {levelButton("error", t("console.errors"), counts.error)}
        <span className="flex-1" />
        {searching ? (
          <label className="relative">
            <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-ink-muted" />
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && (setQuery(""), setSearching(false))}
              placeholder={t("console.search")}
              aria-label={t("console.search")}
              className="h-6 w-48 rounded border border-surface-3 bg-surface-0 pr-6 pl-7 text-[12px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
            <button type="button" onClick={() => (setQuery(""), setSearching(false))} aria-label={t("files.close")} className="absolute top-1/2 right-1 -translate-y-1/2 text-ink-muted hover:text-foreground">
              <X className="size-3.5" />
            </button>
          </label>
        ) : (
          <Button variant="ghost" size="icon" className="size-6" onClick={() => setSearching(true)} aria-label={t("console.search")} title={t("console.search")}>
            <Search />
          </Button>
        )}
        {server.logs.length > 0 && <ShareLogButton content={() => server.logs.join("\n")} />}
      </div>
      <div
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
        className="min-h-0 flex-1 overflow-y-auto px-4 py-3 font-mono text-[12px] leading-relaxed select-text"
      >
        {server.logs.length === 0 ? (
          <p className="font-sans text-[13px] text-ink-muted">{t("hosting.consoleEmpty")}</p>
        ) : shown.length === 0 ? (
          <p className="font-sans text-[13px] text-ink-muted">{t("console.nothing")}</p>
        ) : (
          shown.map((line, i) => <Line key={i} line={line} query={query.trim()} />)
        )}
      </div>
      {footer && <p className="border-t border-surface-3 px-4 py-2 text-[13px] text-destructive">{footer}</p>}
      {suggestions.length > 0 && status !== "stopped" && (
        <div className="flex flex-wrap gap-1 border-t border-surface-3 px-2 pt-2">
          {suggestions.map((s, i) => (
            <button
              key={s}
              type="button"
              onClick={() => setCommand(s + " ")}
              className={cn("rounded bg-surface-2 px-2 py-0.5 font-mono text-[11.5px] text-ink-muted hover:text-foreground", i === 0 && "text-foreground")}
            >
              {s}
              {i === 0 && <span className="ml-1.5 text-[10px] text-ink-muted">Tab</span>}
            </button>
          ))}
        </div>
      )}
      <form
        className="flex gap-2 border-t border-surface-3 p-2"
        onSubmit={(e) => {
          e.preventDefault();
          void send();
        }}
      >
        <Input
          value={command}
          disabled={status === "stopped"}
          onChange={(e) => setCommand(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Tab" && suggestions.length) {
              e.preventDefault();
              setCommand(suggestions[0] + " ");
            } else if (e.key === "ArrowUp" && history.length) {
              e.preventDefault();
              const next = Math.min(history.length - 1, cursor + 1);
              setCursor(next);
              setCommand(history[next]);
            } else if (e.key === "ArrowDown" && cursor >= 0) {
              e.preventDefault();
              const next = cursor - 1;
              setCursor(next);
              setCommand(next < 0 ? "" : history[next]);
            }
          }}
          placeholder={status === "stopped" ? t("hosting.commandOffline") : t("hosting.commandPlaceholder")}
          className="font-mono text-[13px]"
          aria-label={t("hosting.command")}
        />
        <Button type="submit" variant="secondary" size="icon" disabled={status === "stopped" || !command.trim()} aria-label={t("hosting.send")}>
          <Send />
        </Button>
      </form>
    </section>
  );
}

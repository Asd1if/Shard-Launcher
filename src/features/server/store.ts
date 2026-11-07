import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@/lib/backend";
import { onPreviewChange, serverApi, type ServerInfo, type ServerStatus } from "./api";

const MAX_LINES = 2000;

/** Состояние сервера сборки: сведения, консоль и живые события из Rust. */
export function useServer(id: string) {
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [exitCode, setExitCode] = useState<number | null | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    const refresh = () => void serverApi.info(id).then((i) => alive && setInfo(i));
    refresh();
    void serverApi.logs(id).then((l) => alive && setLogs(l));

    if (!isTauri) {
      const off = onPreviewChange((changed) => {
        if (changed !== id) return;
        refresh();
        void serverApi.logs(id).then((l) => alive && setLogs(l));
      });
      return () => {
        alive = false;
        off();
      };
    }

    const subs = [
      listen<{ instanceId: string; line: string }>("server-log", ({ payload }) => {
        if (payload.instanceId === id) setLogs((l) => [...l.slice(-(MAX_LINES - 1)), payload.line]);
      }),
      listen<{ instanceId: string; status: ServerStatus; code: number | null }>("server-status", ({ payload }) => {
        if (payload.instanceId !== id) return;
        if (payload.status === "starting") {
          setLogs([]);
          setExitCode(undefined);
        }
        if (payload.status === "stopped") setExitCode(payload.code);
        refresh();
      }),
      listen<string>("server-players", ({ payload }) => payload === id && refresh()),
    ];
    return () => {
      alive = false;
      subs.forEach((p) => void p.then((off) => off()));
    };
  }, [id]);

  const reload = () => void serverApi.info(id).then(setInfo);
  const addLocal = (line: string) => setLogs((l) => [...l, line]);
  return { info, logs, exitCode, reload, addLocal };
}

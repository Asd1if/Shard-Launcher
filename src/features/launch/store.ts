import { getCurrentWindow } from "@tauri-apps/api/window";
import { backend, isTauri, type CrashReport, type LaunchStage } from "@/lib/backend";
import { createStore } from "@/lib/store";
import { hasAccount, openAccountsDialog } from "@/features/accounts/store";
import { refreshInstances } from "@/features/instances/store";
import { showToast } from "@/components/toast";
import i18n from "@/i18n";
import { errorText } from "@/lib/errors";

export interface LogEntry {
  line: string;
  stderr: boolean;
}

export interface LaunchState {
  status: "idle" | "preparing" | "running";
  stage?: LaunchStage;
  done: number;
  total: number;
  error?: string;
  exitCode?: number | null;
  /** Разбор причины, если игра упала */
  crash?: CrashReport | null;
  logs: LogEntry[];
}

const MAX_LOG_LINES = 3000;
const idle: LaunchState = { status: "idle", done: 0, total: 0, logs: [] };
const store = createStore<Record<string, LaunchState>>({});

function update(id: string, fn: (s: LaunchState) => LaunchState) {
  store.set((all) => ({ ...all, [id]: fn(all[id] ?? idle) }));
}

export function useLaunch(id: string) {
  return store.use()[id] ?? idle;
}

/** Отчёт о вылете, открытый поверх интерфейса */
const crashStore = createStore<{ instanceId: string; name: string; report: CrashReport } | null>(null);
export const useCrashDialog = crashStore.use;
export const openCrash = (instanceId: string, name: string, report: CrashReport) => crashStore.set({ instanceId, name, report });
export const closeCrash = () => crashStore.set(null);

const names = new Map<string, string>();

/** join — сразу подключиться к серверу из настроек сборки */
export async function launch(id: string, name: string, join = false) {
  names.set(id, name);
  if (!hasAccount()) {
    openAccountsDialog();
    return;
  }
  update(id, (s) => ({ ...s, status: "preparing", stage: "version", done: 0, total: 0, error: undefined, exitCode: undefined, crash: undefined, logs: [] }));
  try {
    await backend.launch(id, join);
  } catch (e) {
    const error = errorText(e);
    // Полный текст (например, вывод установщика Forge) — в консоль сборки
    const details = String(e instanceof Error ? e.message : e)
      .split("\n")
      .map((line) => ({ line, stderr: true }));
    update(id, (s) => ({ ...s, status: "idle", error, logs: [...s.logs, ...details] }));
    showToast(i18n.t("launch.failed", { name, error }));
  }
}

export const stop = (id: string) => backend.stop(id);

void backend.on("launch-progress", ({ instanceId, stage, done, total }) =>
  update(instanceId, (s) => ({ ...s, status: "preparing", stage, done, total })),
);
// Свернули окно мы сами — значит, после выхода из игры развернём обратно
let minimizedByLaunch = false;

async function windowOnStart() {
  if (!isTauri) return;
  const { onLaunch } = await backend.settings();
  if (onLaunch === "keep") return;
  const win = getCurrentWindow();
  if (onLaunch === "close") {
    await win.close();
  } else {
    minimizedByLaunch = true;
    await win.minimize();
  }
}

async function windowOnExit() {
  if (!isTauri || !minimizedByLaunch) return;
  minimizedByLaunch = false;
  const win = getCurrentWindow();
  await win.unminimize();
  await win.setFocus();
}

void backend.on("game-started", ({ instanceId }) => {
  update(instanceId, (s) => ({ ...s, status: "running" }));
  void refreshInstances();
  void windowOnStart().catch(() => {});
});
void backend.on("game-log", ({ instanceId, line, stderr }) =>
  update(instanceId, (s) => ({ ...s, logs: [...s.logs.slice(-(MAX_LOG_LINES - 1)), { line, stderr }] })),
);
void backend.on("game-exited", ({ instanceId, code, crash }) => {
  update(instanceId, (s) => ({ ...s, status: "idle", exitCode: code, crash }));
  void windowOnExit().catch(() => {});
  if (crash) openCrash(instanceId, names.get(instanceId) ?? "", crash);
});

// После перезагрузки интерфейса восстанавливаем, какие игры ещё запущены
void backend.running().then((ids) => ids.forEach((id) => update(id, (s) => ({ ...s, status: "running" }))));

import { snapshotBeforeUpdate } from "@/features/instances/snapshots-dialog";
import i18n from "@/i18n";
import { showToast } from "@/components/toast";
import { errorText } from "@/lib/errors";
import { createStore } from "@/lib/store";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "@/lib/backend";
import { refreshInstances } from "@/features/instances/store";
import {
  contentApi,
  projectKey,
  type ContentKind,
  type InstalledContent,
  type InstallResult,
  type ManualDownload,
  type SearchHit,
  type UpdateInfo,
} from "./api";

interface ContentState {
  /** null — ещё не загружали */
  items: InstalledContent[] | null;
  loading: boolean;
  updates: UpdateInfo[] | null;
  checking: boolean;
  /** Файлы и проекты, с которыми сейчас идёт операция */
  busy: string[];
  /** Массовое обновление: сколько из скольких уже готово */
  /** snapshot — снимок перед обновлением (байты), update — обновлено модов из total */
  updating: { done: number; total: number; stage: "snapshot" | "update" } | null;
}

const empty: ContentState = { items: null, loading: false, updates: null, checking: false, busy: [], updating: null };
const store = createStore<Record<string, ContentState>>({});

function update(id: string, fn: (s: ContentState) => ContentState) {
  const all = store.get();
  store.set({ ...all, [id]: fn(all[id] ?? empty) });
}

const setBusy = (id: string, key: string, on: boolean) =>
  update(id, (s) => ({ ...s, busy: on ? [...s.busy, key] : s.busy.filter((k) => k !== key) }));

export function useContent(id: string | null): ContentState {
  const all = store.use();
  return (id && all[id]) || empty;
}

export async function refreshContent(id: string) {
  update(id, (s) => ({ ...s, loading: true }));
  try {
    // Сначала весь список сразу, потом — с названиями только что добавленных файлов
    if (store.get()[id]?.items == null) {
      const quick = await contentApi.list(id, true);
      update(id, (s) => ({ ...s, items: s.items ?? quick }));
    }
    const items = await contentApi.list(id);
    update(id, (s) => ({ ...s, items, loading: false }));
  } catch (e) {
    update(id, (s) => ({ ...s, loading: false }));
    showToast(errorText(e));
  }
}

export function installContent(id: string, kind: ContentKind, hit: SearchHit, anyVersion = false, gameVersion: string | null = null) {
  return runInstall(id, hit, () => contentApi.install(id, hit.projectId, kind, hit.source, hit, anyVersion, gameVersion));
}

/** Конкретная версия со страницы проекта. */
export function installContentVersion(id: string, kind: ContentKind, hit: SearchHit, versionId: string) {
  return runInstall(id, hit, () => contentApi.installVersion(id, kind, hit.source, hit.projectId, versionId, hit));
}

async function runInstall(id: string, hit: SearchHit, run: () => Promise<InstallResult>) {
  const key = projectKey(hit.source, hit.projectId);
  setBusy(id, key, true);
  try {
    const { installed, manual } = await run();
    if (installed.length) {
      const deps = installed.filter((i) => i.dependency).map((i) => i.title);
      showToast(
        deps.length
          ? i18n.t("content.installedWithDeps", { title: hit.title, deps: deps.join(", ") })
          : i18n.t("content.installed", { title: hit.title }),
      );
    }
    if (manual.length) openManual(id, manual);
    await Promise.all([refreshContent(id), refreshInstances()]);
  } catch (e) {
    showToast(i18n.t("content.installFailed", { title: hit.title, error: errorText(e) }));
  } finally {
    setBusy(id, key, false);
  }
}

// ---------- ручная загрузка файлов CurseForge ----------

export interface ManualItem extends ManualDownload {
  done: boolean;
}

interface ManualState {
  instanceId: string;
  items: ManualItem[];
}

const manualStore = createStore<ManualState | null>(null);
export const useManual = () => manualStore.use();

export function openManual(instanceId: string, items: ManualDownload[]) {
  const current = manualStore.get();
  // Новые файлы добавляются к уже открытому диалогу той же сборки
  const kept = current?.instanceId === instanceId ? current.items : [];
  const fresh = items.filter((i) => !kept.some((k) => k.fileName === i.fileName));
  manualStore.set({ instanceId, items: [...kept, ...fresh.map((i) => ({ ...i, done: false }))] });
}

export const closeManual = () => manualStore.set(null);

/** Ищет скачанные файлы в «Загрузках»; вызывается диалогом по таймеру. */
export async function pollManual() {
  const state = manualStore.get();
  if (!state) return;
  let found = false;
  for (const item of state.items.filter((i) => !i.done)) {
    try {
      if (!(await contentApi.manualTake(state.instanceId, item))) continue;
    } catch {
      continue;
    }
    found = true;
    const now = manualStore.get();
    if (now?.instanceId !== state.instanceId) return;
    manualStore.set({ ...now, items: now.items.map((i) => (i.fileName === item.fileName ? { ...i, done: true } : i)) });
  }
  if (found) await Promise.all([refreshContent(state.instanceId), refreshInstances()]);
}

export async function toggleContent(id: string, item: InstalledContent) {
  setBusy(id, item.file, true);
  try {
    const file = await contentApi.toggle(id, item.kind, item.file, !item.enabled);
    update(id, (s) => ({
      ...s,
      items: s.items?.map((c) => (c.file === item.file && c.kind === item.kind ? { ...c, file, enabled: !item.enabled } : c)) ?? null,
      updates: s.updates?.map((u) => (u.file === item.file && u.kind === item.kind ? { ...u, file } : u)) ?? null,
    }));
    void refreshInstances();
  } catch (e) {
    showToast(errorText(e));
  } finally {
    setBusy(id, item.file, false);
  }
}

export async function removeContent(id: string, item: InstalledContent) {
  setBusy(id, item.file, true);
  try {
    await contentApi.remove(id, item.kind, item.file);
    update(id, (s) => ({
      ...s,
      items: s.items?.filter((c) => !(c.file === item.file && c.kind === item.kind)) ?? null,
      updates: s.updates?.filter((u) => !(u.file === item.file && u.kind === item.kind)) ?? null,
    }));
    void refreshInstances();
  } catch (e) {
    showToast(errorText(e));
  } finally {
    setBusy(id, item.file, false);
  }
}

export interface MissingDep {
  id: string;
  forMods: string[];
}

/** Обязательные зависимости, которых не хватает модам сборки. */
export const missingDeps = (id: string) =>
  isTauri ? invoke<MissingDep[]>("content_missing_deps", { id }) : Promise.resolve<MissingDep[]>([{ id: "fabric-api", forMods: ["Sodium Extra"] }]);

export async function installDeps(id: string, ids: string[]) {
  const result = isTauri
    ? await invoke<{ done: string[]; failed: string[] }>("content_install_deps", { id, ids })
    : { done: ids, failed: [] as string[] };
  await Promise.all([refreshContent(id), refreshInstances()]);
  return result;
}

export async function checkUpdates(id: string) {
  update(id, (s) => ({ ...s, checking: true }));
  try {
    const updates = await contentApi.updates(id);
    update(id, (s) => ({ ...s, updates, checking: false }));
    if (updates.length === 0) showToast(i18n.t("content.upToDate"));
  } catch (e) {
    update(id, (s) => ({ ...s, checking: false }));
    showToast(errorText(e));
  }
}

export async function applyUpdates(id: string, list: UpdateInfo[]) {
  // Перед несколькими обновлениями — снимок сборки, чтобы можно было откатиться
  if (list.length > 1) {
    update(id, (s) => ({ ...s, updating: { done: 0, total: 0, stage: "snapshot" } }));
    await snapshotBeforeUpdate(id, (p) => update(id, (s) => ({ ...s, updating: { done: p.done, total: p.total, stage: "snapshot" } })));
  }
  for (const u of list) setBusy(id, u.file, true);
  const total = list.length;
  let done = 0;
  update(id, (s) => ({ ...s, updating: { done, total, stage: "update" } }));
  let failed = 0;
  let lastError: unknown = null;
  for (const u of list) {
    try {
      await contentApi.update(id, u);
      update(id, (s) => ({ ...s, updates: s.updates?.filter((x) => !(x.file === u.file && x.kind === u.kind)) ?? null }));
    } catch (e) {
      failed += 1;
      lastError = e;
    } finally {
      setBusy(id, u.file, false);
      done += 1;
      update(id, (s) => ({ ...s, updating: { done, total, stage: "update" } }));
    }
  }
  update(id, (s) => ({ ...s, updating: null }));
  if (failed) showToast(`${i18n.t("content.updateFailed", { count: failed })} ${errorText(lastError)}`);
  await refreshContent(id);
}

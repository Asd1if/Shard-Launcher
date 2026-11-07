import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "@/lib/backend";

export interface CpmModel {
  /** Имя файла .cpmmodel — так же он называется в player_models сборки */
  id: string;
  name: string;
  size: number;
  addedAt: string;
}

/** Проект мода Customizable Player Models на Modrinth */
export const CPM_MOD = "custom-player-models";

let previewList: CpmModel[] = [{ id: "Knight.cpmmodel", name: "Knight", size: 18_432, addedAt: new Date().toISOString() }];
let previewInstalled: [string, string][] = [];

export const cpmApi = {
  list: () => (isTauri ? invoke<CpmModel[]>("cpm_list") : Promise.resolve(previewList)),
  add: (path: string) => invoke<string>("cpm_add", { path }),
  remove: async (id: string) => {
    if (isTauri) return invoke<void>("cpm_remove", { id });
    previewList = previewList.filter((m) => m.id !== id);
  },
  /** [id сборки, файл модели] */
  installed: () => (isTauri ? invoke<[string, string][]>("cpm_installed") : Promise.resolve(previewInstalled)),
  install: async (id: string, instanceId: string) => {
    if (isTauri) return invoke<void>("cpm_install", { id, instanceId });
    previewInstalled = [...previewInstalled.filter(([i, f]) => !(i === instanceId && f === id)), [instanceId, id]];
  },
  uninstall: async (id: string, instanceId: string) => {
    if (isTauri) return invoke<void>("cpm_uninstall", { id, instanceId });
    previewInstalled = previewInstalled.filter(([i, f]) => !(i === instanceId && f === id));
  },
  open: () => (isTauri ? invoke<void>("cpm_open") : Promise.resolve()),
};

/** Выбор файлов .cpmmodel. */
export async function pickCpmModels(title: string): Promise<string[]> {
  if (!isTauri) return [];
  const { open } = await import("@tauri-apps/plugin-dialog");
  const paths = await open({ title, multiple: true, filters: [{ name: "CPM", extensions: ["cpmmodel"] }] });
  return Array.isArray(paths) ? paths : typeof paths === "string" ? [paths] : [];
}

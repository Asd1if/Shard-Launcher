import { backend, type InstanceUpdate, type NewInstance } from "@/lib/backend";
import { createStore } from "@/lib/store";
import type { ByteProgress } from "@/components/ui/progress-bar";
import type { Instance } from "./types";

const store = createStore<Instance[]>([]);

export async function refreshInstances() {
  store.set(await backend.instances());
}

export async function createInstance(data: NewInstance) {
  const instance = await backend.createInstance(data);
  await refreshInstances();
  return instance;
}

export async function updateInstance(id: string, update: InstanceUpdate) {
  const instance = await backend.updateInstance(id, update);
  await refreshInstances();
  return instance;
}

export async function setInstanceIcon(id: string, icon: string | null) {
  await backend.setInstanceIcon(id, icon);
  await refreshInstances();
}

export async function setInstanceMeta(id: string, group: string | null, favorite: boolean) {
  await backend.setInstanceMeta(id, group, favorite);
  await refreshInstances();
}

export async function cloneInstance(id: string, name: string, onProgress?: (p: ByteProgress) => void) {
  const copy = await backend.cloneInstance(id, name, onProgress);
  await refreshInstances();
  return copy;
}

export async function deleteInstance(id: string, onProgress?: (p: ByteProgress) => void) {
  await backend.deleteInstance(id, onProgress);
  await refreshInstances();
}

export const useInstances = store.use;

/** Сначала недавно запущенные, потом новые. */
export function byLastPlayed(a: Instance, b: Instance) {
  return (b.lastPlayed ?? b.createdAt).localeCompare(a.lastPlayed ?? a.createdAt);
}

void refreshInstances();

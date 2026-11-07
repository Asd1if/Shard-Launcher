import { useEffect, useSyncExternalStore } from "react";
import type { Account } from "@/lib/backend";
import { skinsApi } from "@/features/skins/api";

// Лица лицензионных аккаунтов (8×8 из скина, с вторым слоем) — в памяти и в localStorage,
// чтобы голова в меню появлялась сразу, а не после запроса к Mojang.
const faces = new Map<string, string | null>();
const listeners = new Set<() => void>();
const loading = new Set<string>();
const storageKey = (uuid: string) => `shard.face.${uuid}`;

function read(uuid: string): string | null | undefined {
  if (faces.has(uuid)) return faces.get(uuid);
  try {
    const saved = localStorage.getItem(storageKey(uuid));
    if (saved) faces.set(uuid, saved);
    return saved ?? undefined;
  } catch {
    return undefined;
  }
}

function write(uuid: string, face: string | null) {
  faces.set(uuid, face);
  try {
    if (face) localStorage.setItem(storageKey(uuid), face);
    else localStorage.removeItem(storageKey(uuid));
  } catch {
    // без хранилища — только в памяти
  }
  listeners.forEach((l) => l());
}

/** Лицо из текстуры скина: голова (8,8) и поверх — второй слой (40,8). */
export function faceFromSkin(src: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = 8;
      const ctx = canvas.getContext("2d")!;
      ctx.drawImage(img, 8, 8, 8, 8, 0, 0, 8, 8);
      ctx.drawImage(img, 40, 8, 8, 8, 0, 0, 8, 8);
      resolve(canvas.toDataURL());
    };
    img.onerror = () => reject(new Error("skin_not_png"));
    img.src = src;
  });
}

/** Обновить лицо после смены скина (страница скинов знает текстуру и так). */
export async function setAccountSkin(uuid: string, skin: string) {
  write(uuid, await faceFromSkin(skin));
}

async function load(account: Account) {
  if (loading.has(account.uuid)) return;
  loading.add(account.uuid);
  try {
    const profile = await skinsApi.profile(account.id);
    const active = profile.skins.find((s) => s.state === "ACTIVE");
    if (active) await setAccountSkin(account.uuid, await skinsApi.texture(active.url));
  } catch {
    // нет сети или токен устарел — останется сохранённое лицо или нарисованное
  } finally {
    loading.delete(account.uuid);
  }
}

const loaded = new Set<string>();

/** Лицо лицензионного аккаунта; null — рисуем голову по нику. Раз за запуск сверяется с Mojang. */
export function useAccountFace(account: Account | null | undefined): string | null {
  const microsoft = account?.kind === "microsoft" ? account : null;
  const face = useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => (microsoft ? (read(microsoft.uuid) ?? null) : null),
  );
  useEffect(() => {
    if (!microsoft || loaded.has(microsoft.uuid)) return;
    loaded.add(microsoft.uuid);
    void load(microsoft);
  }, [microsoft?.uuid]);
  return face;
}

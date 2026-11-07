import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "@/lib/backend";

export type SkinModel = "classic" | "slim";

export interface Skin {
  id: string;
  name: string;
  model: SkinModel;
  sha1: string;
  addedAt: string;
}

export interface ProfileSkin {
  id: string;
  state: string;
  url: string;
  variant: string | null;
}

export interface ProfileCape {
  id: string;
  state: string;
  url: string;
  alias: string | null;
}

export interface Profile {
  id: string;
  name: string;
  skins: ProfileSkin[];
  capes: ProfileCape[];
}

// ---------- кэш профиля по аккаунту: плащи и скин видны сразу, без ожидания Mojang ----------

interface CachedProfile {
  profile: Profile;
  /** url текстуры → data URL */
  textures: Record<string, string>;
}

const profileKey = (accountId: string) => `shard.profile.${accountId}`;

export function cachedProfile(accountId: string): CachedProfile | null {
  try {
    const raw = localStorage.getItem(profileKey(accountId));
    return raw ? (JSON.parse(raw) as CachedProfile) : null;
  } catch {
    return null;
  }
}

export function saveProfile(accountId: string, profile: Profile, textures: Record<string, string>) {
  try {
    localStorage.setItem(profileKey(accountId), JSON.stringify({ profile, textures }));
  } catch {
    // переполнено или недоступно — обойдёмся без кэша
  }
}

// ---------- превью интерфейса без Rust ----------

const preview = { skins: [] as Skin[], data: new Map<string, string>(), profile: null as Profile | null };

/** Плащ-заглушка 64×32 для превью: однотонный с полосой. */
function previewCape(base: string, stripe: string) {
  const c = document.createElement("canvas");
  c.width = 64;
  c.height = 32;
  const ctx = c.getContext("2d")!;
  ctx.fillStyle = base;
  ctx.fillRect(0, 0, 64, 32);
  ctx.fillStyle = stripe;
  ctx.fillRect(1, 6, 10, 3);
  ctx.fillRect(34, 8, 12, 3);
  return c.toDataURL();
}

/** Скин-заглушка 64×64: голова, тело, руки и ноги разного цвета. */
function previewSkin() {
  const c = document.createElement("canvas");
  c.width = c.height = 64;
  const ctx = c.getContext("2d")!;
  const fill = (color: string, x: number, y: number, w: number, h: number) => {
    ctx.fillStyle = color;
    ctx.fillRect(x, y, w, h);
  };
  fill("#c99a74", 0, 0, 32, 16);
  fill("#3f8fd8", 16, 16, 24, 16);
  fill("#3f8fd8", 32, 48, 16, 16);
  fill("#c99a74", 40, 16, 16, 16);
  fill("#2e3f73", 0, 16, 16, 16);
  fill("#2e3f73", 16, 48, 16, 16);
  return c.toDataURL();
}

function previewProfile(): Profile {
  return (preview.profile ??= {
    id: "preview",
    name: "Preview",
    skins: [{ id: "skin", state: "ACTIVE", url: previewSkin(), variant: "CLASSIC" }],
    capes: [
      { id: "migrator", state: "ACTIVE", url: previewCape("#3b6fd1", "#f0c040"), alias: "Migrator" },
      { id: "pan", state: "INACTIVE", url: previewCape("#c0503a", "#ffffff"), alias: "Pan" },
      { id: "15th", state: "INACTIVE", url: previewCape("#2f8f5a", "#d9f2e3"), alias: "15th Anniversary" },
    ],
  });
}

async function sha1(text: string) {
  const digest = await crypto.subtle.digest("SHA-1", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function previewAdd(data: string, name: string, model: SkinModel): Promise<Skin> {
  const hash = await sha1(data);
  const existing = preview.skins.find((s) => s.sha1 === hash);
  if (existing) return existing;
  const skin: Skin = { id: crypto.randomUUID(), name: name.trim() || "Skin", model, sha1: hash, addedAt: new Date().toISOString() };
  preview.skins = [skin, ...preview.skins];
  preview.data.set(skin.id, data);
  return skin;
}

export const skinsApi = {
  list: () => (isTauri ? invoke<Skin[]>("skins_list") : Promise.resolve(preview.skins)),

  data: (id: string) =>
    isTauri ? invoke<string>("skin_data", { id }) : Promise.resolve(preview.data.get(id) ?? ""),

  /** data — PNG в виде data URL */
  addData: (data: string, name: string, model: SkinModel) =>
    isTauri ? invoke<Skin>("skin_add_data", { data, name, model }) : previewAdd(data, name, model),

  addPlayer: (name: string) =>
    isTauri
      ? invoke<Skin>("skin_add_player", { name })
      : Promise.reject(new Error("skin_preview_unavailable")),

  update: async (id: string, name: string, model: SkinModel) => {
    if (isTauri) return invoke<Skin>("skin_update", { id, name, model });
    preview.skins = preview.skins.map((s) => (s.id === id ? { ...s, name: name.trim() || s.name, model } : s));
    return preview.skins.find((s) => s.id === id)!;
  },

  remove: async (id: string) => {
    if (isTauri) return invoke<void>("skin_remove", { id });
    preview.skins = preview.skins.filter((s) => s.id !== id);
  },

  export: (id: string, path: string) => invoke<void>("skin_export", { id, path }),

  profile: (accountId: string) => (isTauri ? invoke<Profile>("skin_profile", { accountId }) : Promise.resolve(structuredClone(previewProfile()))),

  texture: (url: string) => (isTauri ? invoke<string>("skin_texture", { url }) : Promise.resolve(url)),

  apply: (accountId: string, id: string) => invoke<Profile>("skin_apply", { accountId, id }),

  setCape: async (accountId: string, capeId: string | null) => {
    if (isTauri) return invoke<Profile>("skin_set_cape", { accountId, capeId });
    const p = previewProfile();
    p.capes = p.capes.map((c) => ({ ...c, state: c.id === capeId ? "ACTIVE" : "INACTIVE" }));
    return structuredClone(p);
  },
};

/** Сохранить PNG скина: окно выбора файла, затем копия из библиотеки. */
export async function exportSkin(skin: Skin, title: string) {
  if (!isTauri) {
    const a = document.createElement("a");
    a.href = await skinsApi.data(skin.id);
    a.download = `${skin.name}.png`;
    a.click();
    return;
  }
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ title, defaultPath: `${skin.name}.png`, filters: [{ name: "PNG", extensions: ["png"] }] });
  if (path) await skinsApi.export(skin.id, path);
}

/** Читает выбранный файл как data URL. */
export function readFile(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

export function loadImage(src: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("skin_not_png"));
    img.src = src;
  });
}

/**
 * Тонкие руки (Alex) у скина 64×64: у Steve столбцы 54–55 развёртки правой руки заняты,
 * у Alex рука на пиксель уже и там пусто. Та же проверка, что в skinview-utils.
 */
export function detectModel(img: HTMLImageElement): SkinModel {
  if (img.height !== 64) return "classic";
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const ctx = canvas.getContext("2d")!;
  ctx.drawImage(img, 0, 0);
  const alpha = ctx.getImageData(54, 20, 2, 12).data;
  for (let i = 3; i < alpha.length; i += 4) if (alpha[i] !== 0) return "classic";
  return "slim";
}

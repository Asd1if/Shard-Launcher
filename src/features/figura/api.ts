import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "@/lib/backend";

export interface Avatar {
  id: string;
  name: string;
  description: string;
  authors: string[];
  color: string | null;
  models: string[];
  scripts: number;
  size: number;
  addedAt: string;
  /** Имя папки в figura/avatars сборки */
  folder: string;
}

export interface AvatarFiles {
  /** [путь, JSON модели Blockbench] */
  models: [string, string][];
  /** [путь, data URL] */
  textures: [string, string][];
}

// ---------- превью интерфейса без Rust: один встроенный аватар-кубик ----------

const previewModel = JSON.stringify({
  meta: { format_version: "4.5", model_format: "free", box_uv: false },
  resolution: { width: 16, height: 16 },
  elements: [
    { name: "body", type: "cube", from: [-4, 0, -4], to: [4, 8, 4], origin: [0, 0, 0], faces: face(0), uuid: "c1" },
    { name: "ear_l", type: "cube", from: [-4, 8, -1], to: [-2, 11, 1], origin: [-3, 8, 0], faces: face(0), uuid: "c2" },
    { name: "ear_r", type: "cube", from: [2, 8, -1], to: [4, 11, 1], origin: [3, 8, 0], faces: face(0), uuid: "c3" },
  ],
  outliner: [{ name: "Head", origin: [0, 0, 0], rotation: [0, 0, 0], uuid: "g1", children: ["c1", { name: "Ears", origin: [0, 8, 0], uuid: "g2", children: ["c2", "c3"] }] }],
  textures: [{ name: "skin", source: checker(), uv_width: 16, uv_height: 16 }],
  animations: [
    {
      name: "bounce",
      loop: "loop",
      length: 1,
      animators: {
        g1: { name: "Head", type: "bone", keyframes: [kf("position", 0, 0), kf("position", 0.5, 2), kf("position", 1, 0)] },
        g2: { name: "Ears", type: "bone", keyframes: [kf("rotation", 0, 0, 0, 0), kf("rotation", 0.5, 0, 0, 15), kf("rotation", 1, 0, 0, 0)] },
      },
    },
  ],
});

function face(texture: number) {
  const f = { uv: [0, 0, 16, 16], texture };
  return { north: f, south: f, east: f, west: f, up: f, down: f };
}

function kf(channel: string, time: number, y: number, x = 0, z = 0) {
  return { channel, time, interpolation: "linear", data_points: [{ x, y, z }] };
}

function checker() {
  const c = document.createElement("canvas");
  c.width = c.height = 16;
  const ctx = c.getContext("2d")!;
  for (let y = 0; y < 16; y += 2) for (let x = 0; x < 16; x += 2) {
    ctx.fillStyle = (x + y) % 4 ? "#f0a040" : "#c06020";
    ctx.fillRect(x, y, 2, 2);
  }
  return c.toDataURL();
}

let previewList: Avatar[] = [
  {
    id: "preview",
    name: "Preview Fox",
    description: "Preview data",
    authors: ["Shard"],
    color: "#f0a040",
    models: ["fox"],
    scripts: 1,
    size: 4096,
    addedAt: new Date().toISOString(),
    folder: "Preview Fox",
  },
];

export const figuraApi = {
  list: () => (isTauri ? invoke<Avatar[]>("figura_list") : Promise.resolve(previewList)),
  add: (path: string) => invoke<Avatar>("figura_add", { path }),
  remove: async (id: string) => {
    if (isTauri) return invoke<void>("figura_remove", { id });
    previewList = previewList.filter((a) => a.id !== id);
  },
  rename: (id: string, name: string) => invoke<Avatar>("figura_rename", { id, name }),
  files: (id: string) =>
    isTauri ? invoke<AvatarFiles>("figura_files", { id }) : Promise.resolve<AvatarFiles>({ models: [["fox.bbmodel", previewModel]], textures: [] }),
  /** [id сборки, папка аватара] */
  installed: () => (isTauri ? invoke<[string, string][]>("figura_installed") : Promise.resolve<[string, string][]>([])),
  install: (id: string, instanceId: string) => (isTauri ? invoke<void>("figura_install", { id, instanceId }) : Promise.resolve()),
  uninstall: (id: string, instanceId: string) => (isTauri ? invoke<void>("figura_uninstall", { id, instanceId }) : Promise.resolve()),
  open: (id: string) => (isTauri ? invoke<void>("figura_open", { id }) : Promise.resolve()),
};

/** Выбор аватара: zip-архив или папка. */
export async function pickAvatar(title: string, folder: boolean): Promise<string | null> {
  if (!isTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const path = await open(
    folder ? { title, directory: true, multiple: false } : { title, multiple: false, filters: [{ name: "Figura", extensions: ["zip"] }] },
  );
  return typeof path === "string" ? path : null;
}

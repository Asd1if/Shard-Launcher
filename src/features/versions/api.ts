import { invoke } from "@tauri-apps/api/core";
import type { VersionType } from "@/features/instances/types";

export interface McVersion {
  id: string;
  type: VersionType;
  url: string;
  releaseTime: string;
}

export interface VersionManifest {
  latest: { release: string; snapshot: string };
  versions: McVersion[];
}

const isTauri = "__TAURI_INTERNALS__" in window;

/** Манифест версий Mojang. В Tauri идёт через Rust (с кешем), в обычном браузере — напрямую. */
export async function fetchVersions(): Promise<VersionManifest> {
  if (isTauri) return invoke<VersionManifest>("minecraft_versions");
  const res = await fetch("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json");
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

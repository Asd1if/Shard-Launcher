import { useEffect } from "react";
import { useLocation } from "react-router";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "@/lib/backend";
import { useInstances } from "@/features/instances/store";

/** Какой раздел лаунчера открыт — для статуса Discord. */
function page(path: string): string {
  if (path.startsWith("/instances/")) return "instance";
  if (path.startsWith("/instances")) return "instances";
  if (path.startsWith("/modpacks")) return "modpacks";
  if (path.startsWith("/skins")) return "skins";
  if (path.startsWith("/settings")) return "settings";
  return "home";
}

/**
 * Статус «Играет в Shard» в Discord, пока открыт лаунчер: что сейчас открыто.
 * Во время игры Rust ставит статус игры поверх, после выхода возвращает этот.
 */
export function useDiscordPresence() {
  const { t, i18n } = useTranslation();
  const { pathname } = useLocation();
  const instances = useInstances();
  const where = page(pathname);
  const instanceName = where === "instance" ? (instances.find((i) => pathname.endsWith(`/${i.id}`))?.name ?? "") : "";

  useEffect(() => {
    if (!isTauri) return;
    // Discord пускает не больше 5 обновлений за 20 секунд — ждём, пока человек остановится
    const timer = setTimeout(() => {
      invoke("discord_idle", { details: t(`discord.${where}`), state: instanceName || t("discord.idle") }).catch(() => {});
    }, 1500);
    return () => clearTimeout(timer);
  }, [where, instanceName, i18n.language, t]);
}

import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import type { Instance, VersionType } from "./types";

/** «Fabric 1.21.4» или «Vanilla 1.21.4» — ядро и загрузчик одной строкой. */
export function useInstanceLabel() {
  const { t } = useTranslation();
  return (inst: Instance) =>
    `${inst.loader ? t(`loader.${inst.loader}`) : t("core.vanilla")} ${inst.mcVersion}`;
}

/** «Minecraft 1.20.1, Forge 47.3.0» — для шапки сборки, где есть место. */
export function useInstanceDetails() {
  const { t } = useTranslation();
  return (inst: Instance) => {
    const core = inst.loader
      ? [t(`loader.${inst.loader}`), inst.loaderVersion].filter(Boolean).join(" ")
      : t("core.vanilla");
    return `Minecraft ${inst.mcVersion}, ${core}`;
  };
}

/** Метка для нерелизных версий: снапшот, бета, альфа. Для релизов ничего не рисует. */
export function VersionTypeBadge({ type, className }: { type: VersionType; className?: string }) {
  const { t } = useTranslation();
  if (type === "release") return null;
  return (
    <span
      className={cn(
        "inline-flex h-[18px] shrink-0 items-center rounded px-1.5 text-[11px] leading-none",
        type === "snapshot" ? "bg-[#f5a524]/15 text-[#e0951a]" : "bg-surface-3 text-ink-muted",
        className,
      )}
    >
      {t(`versionType.${type}`)}
    </span>
  );
}

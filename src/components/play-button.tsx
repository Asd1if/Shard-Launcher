import { Play, Square } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { launch, stop, useLaunch } from "@/features/launch/store";

/**
 * Главная кнопка лаунчера: пиксельные углы и блик при наведении.
 * Во время подготовки показывает прогресс заливкой, во время игры — «Остановить».
 */
export function PlayButton({
  instanceId,
  instanceName,
  size = "lg",
  className,
}: {
  instanceId: string;
  instanceName: string;
  size?: "lg" | "sm";
  className?: string;
}) {
  const { t } = useTranslation();
  const state = useLaunch(instanceId);
  const percent = state.total > 0 ? Math.floor((state.done / state.total) * 100) : 0;
  const lg = size === "lg";

  if (state.status === "preparing") {
    return (
      <div
        role="progressbar"
        aria-valuenow={percent}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={t("launch.preparing")}
        className={cn(
          "pixel-corners relative inline-flex items-center justify-center overflow-hidden bg-surface-3 font-display font-bold text-foreground",
          lg ? "h-14 min-w-[220px] px-6 text-sm [--step:4px]" : "h-9 min-w-[110px] px-3 text-[11px] [--step:2px]",
          className,
        )}
      >
        <span
          aria-hidden
          className="absolute inset-y-0 left-0 bg-brand/35 transition-[width] duration-300"
          style={{ width: `${percent}%` }}
        />
        <span className="relative tabular-nums">
          {state.stage ? t(`launch.stage.${state.stage}`) : t("launch.preparing")}
          {state.total > 0 && ` ${percent}%`}
        </span>
      </div>
    );
  }

  if (state.status === "running") {
    return (
      <button
        type="button"
        onClick={() => void stop(instanceId)}
        className={cn(
          "pixel-corners inline-flex items-center justify-center gap-3 bg-surface-3 font-display font-bold text-foreground outline-none transition-colors hover:bg-destructive hover:text-white focus-visible:ring-2 focus-visible:ring-ring",
          lg ? "h-14 px-9 text-lg [--step:4px]" : "h-9 px-4 text-xs [--step:2px]",
          className,
        )}
      >
        <Square className={cn("fill-current", lg ? "size-4" : "size-3")} />
        {t("launch.stop")}
      </button>
    );
  }

  return (
    <button
      type="button"
      onClick={() => void launch(instanceId, instanceName)}
      className={cn(
        "pixel-corners enchant-glint inline-flex items-center justify-center gap-3 bg-brand font-display font-bold text-brand-foreground outline-none transition-[filter] hover:brightness-110 active:brightness-95 focus-visible:brightness-110",
        lg ? "h-14 px-9 text-lg [--step:4px]" : "h-9 px-4 text-xs [--step:2px]",
        className,
      )}
    >
      <Play className={cn("fill-current", lg ? "size-5" : "size-3.5")} />
      {t("home.play")}
    </button>
  );
}

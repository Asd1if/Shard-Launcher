import { useTranslation } from "react-i18next";
import { formatSize } from "@/lib/format";
import { cn } from "@/lib/utils";

/** Прогресс в байтах от Rust. */
export interface ByteProgress {
  done: number;
  total: number;
}

/**
 * Полоска с «1,2 GB из 3,4 GB» (или «12 из 40» для `unit="count"`). Пока размер не известен —
 * бегущая полоска.
 */
export function ProgressBar({
  progress,
  label,
  unit = "bytes",
  className,
}: {
  progress: ByteProgress | null;
  label?: string;
  unit?: "bytes" | "count";
  className?: string;
}) {
  const { t, i18n } = useTranslation();
  const percent = progress && progress.total > 0 ? Math.min(100, (progress.done / progress.total) * 100) : null;
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <div className="flex items-baseline gap-2 text-xs text-ink-muted">
        {label && <span className="min-w-0 truncate">{label}</span>}
        {progress && percent !== null && (
          <span className="ml-auto shrink-0 tabular-nums">
            {unit === "bytes"
              ? t("progress.bytes", { done: formatSize(progress.done, i18n.language), total: formatSize(progress.total, i18n.language) })
              : t("progress.bytes", { done: progress.done, total: progress.total })}{" "}
            ·{" "}
            {Math.floor(percent)}%
          </span>
        )}
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-surface-3" role="progressbar" aria-valuenow={percent ?? undefined} aria-valuemin={0} aria-valuemax={100}>
        <div
          className={cn("h-full rounded-full bg-brand transition-[width] duration-150", percent === null && "w-1/3 animate-pulse")}
          style={percent === null ? undefined : { width: `${percent}%` }}
        />
      </div>
    </div>
  );
}

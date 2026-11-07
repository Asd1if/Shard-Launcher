import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Loader2 } from "lucide-react";
import { backend, type LoaderVersion } from "@/lib/backend";
import { cn } from "@/lib/utils";
import type { ModLoader } from "./types";

/**
 * Выбор версии загрузчика. Пустое значение — «рекомендованная»: лаунчер выберет её при запуске.
 * `allowAuto=false` — для уже созданной сборки, где версия известна.
 */
export function LoaderVersionSelect({
  id,
  loader,
  mcVersion,
  value,
  onChange,
  allowAuto = true,
  className,
}: {
  id?: string;
  loader: ModLoader;
  mcVersion: string;
  value: string | null;
  onChange: (version: string | null) => void;
  allowAuto?: boolean;
  className?: string;
}) {
  const { t } = useTranslation();
  const [list, setList] = useState<LoaderVersion[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [showUnstable, setShowUnstable] = useState(false);

  useEffect(() => {
    let alive = true;
    setList(null);
    setFailed(false);
    backend
      .loaderVersions(loader, mcVersion)
      .then((l) => alive && setList(l))
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
    };
  }, [loader, mcVersion]);

  if (failed) return <p className="text-xs text-destructive">{t("loaderVersion.loadError")}</p>;
  if (!list)
    return (
      <p className="flex h-9 items-center gap-2 text-xs text-ink-muted">
        <Loader2 className="size-3.5 animate-spin" /> {t("loaderVersion.loading")}
      </p>
    );
  if (list.length === 0) return <p className="text-xs text-destructive">{t("loaderVersion.none", { version: mcVersion })}</p>;

  const recommended = list.find((v) => v.recommended) ?? list.find((v) => v.stable) ?? list[0];
  const hasUnstable = list.some((v) => !v.stable);
  // Выбранная нестабильная версия видна, даже если фильтр скрывает остальные
  const shown = list.filter((v) => v.stable || showUnstable || v.version === value);

  return (
    <div className={cn("flex flex-wrap items-center gap-3", className)}>
      <div className="relative">
        <select
          id={id}
          value={value ?? ""}
          onChange={(e) => onChange(e.target.value || null)}
          className="h-9 min-w-[220px] appearance-none rounded-md border border-input bg-surface-0 pr-9 pl-3 text-[13px] tabular-nums outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
        >
          {allowAuto && (
            <option value="">{t("loaderVersion.auto", { version: recommended.version })}</option>
          )}
          {shown.map((v) => (
            <option key={v.version} value={v.version}>
              {v.version}
              {v.recommended ? ` (${t("loaderVersion.recommended")})` : !v.stable ? ` (${t("loaderVersion.unstable")})` : ""}
            </option>
          ))}
        </select>
        <ChevronDown className="pointer-events-none absolute top-1/2 right-3 size-3.5 -translate-y-1/2 text-ink-muted" />
      </div>
      {hasUnstable && (
        <label className="flex items-center gap-2 text-xs text-ink-muted">
          <input
            type="checkbox"
            checked={showUnstable}
            onChange={(e) => setShowUnstable(e.target.checked)}
            className="size-3.5 accent-[var(--brand)]"
          />
          {t("loaderVersion.showUnstable")}
        </label>
      )}
    </div>
  );
}

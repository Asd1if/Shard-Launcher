import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { showToast } from "@/components/toast";
import { contentApi } from "@/features/content/api";
import { refreshContent } from "@/features/content/store";
import { InstanceIcon } from "@/features/instances/instance-icon";
import { refreshInstances, useInstances } from "@/features/instances/store";
import { errorText } from "@/lib/errors";

/** Кнопка «Установить» со списком сборок с загрузчиком модов. */
export function InstallMenu({
  installedIn,
  busy,
  onInstall,
}: {
  installedIn: string[];
  busy: boolean;
  onInstall: (instanceId: string) => void;
}) {
  const { t } = useTranslation();
  // Модели игрока имеют смысл только там, где есть загрузчик модов
  const targets = useInstances().filter((i) => i.loader);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button className="flex-1" disabled={busy}>
          {busy ? <Loader2 className="animate-spin" /> : <Plus />} {t("figura.install")}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-64 border-surface-3 bg-surface-1">
        <DropdownMenuLabel className="text-xs text-ink-muted">{t("figura.installTo")}</DropdownMenuLabel>
        {targets.length === 0 && <p className="px-2 py-1.5 text-xs text-ink-muted">{t("figura.noTargets")}</p>}
        {targets.map((inst) => (
          <DropdownMenuItem key={inst.id} onSelect={() => onInstall(inst.id)} className="gap-2.5">
            <InstanceIcon name={inst.name} loader={inst.loader} icon={inst.icon} className="size-6" />
            <span className="min-w-0 flex-1 truncate">{inst.name}</span>
            {installedIn.includes(inst.id) && <span className="text-[11px] text-ink-muted">{t("figura.reinstall")}</span>}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Сборки, где стоит модель; для сборок без нужного мода — кнопка «Добавить мод». */
export function InstalledIn({
  ids,
  mod,
  modPattern,
  onUninstall,
}: {
  ids: string[];
  /** Проект мода на Modrinth */
  mod: string;
  /** Как опознать мод в списке контента сборки */
  modPattern: RegExp;
  onUninstall: (instanceId: string) => void;
}) {
  const { t } = useTranslation();
  const instances = useInstances();
  const [withoutMod, setWithoutMod] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void Promise.all(
      ids.map(async (id) => {
        const items = await contentApi.list(id).catch(() => []);
        const has = items.some((c) => c.kind === "mod" && modPattern.test(`${c.meta?.title ?? ""} ${c.file}`));
        return has ? null : id;
      }),
    ).then((list) => !cancelled && setWithoutMod(new Set(list.filter((x): x is string => !!x))));
    return () => {
      cancelled = true;
    };
  }, [ids.join(",")]);

  const installMod = async (instanceId: string) => {
    setBusy(instanceId);
    try {
      await contentApi.install(instanceId, mod, "mod", "modrinth");
      await Promise.all([refreshContent(instanceId), refreshInstances()]);
      setWithoutMod((s) => new Set([...s].filter((id) => id !== instanceId)));
      showToast(t("figura.modInstalled"));
    } catch (e) {
      showToast(t("figura.modFailed", { error: errorText(e) }));
    } finally {
      setBusy(null);
    }
  };

  if (ids.length === 0) return null;
  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-xs text-ink-muted">{t("figura.installedIn")}</p>
      <ul className="flex flex-col gap-1">
        {ids.map((id) => {
          const inst = instances.find((i) => i.id === id);
          if (!inst) return null;
          return (
            <li key={id} className="flex items-center gap-2 rounded-md bg-surface-2 py-1 pr-1 pl-2 text-[13px]">
              <InstanceIcon name={inst.name} loader={inst.loader} icon={inst.icon} className="size-5" />
              <span className="min-w-0 flex-1 truncate">{inst.name}</span>
              {withoutMod.has(id) && (
                <Button
                  size="sm"
                  variant="secondary"
                  className="h-6 px-2 text-[11px]"
                  disabled={busy === id}
                  onClick={() => void installMod(id)}
                  title={t("figura.noModHint")}
                >
                  {busy === id ? <Loader2 className="animate-spin" /> : null} {t("figura.addMod")}
                </Button>
              )}
              <button
                type="button"
                onClick={() => onUninstall(id)}
                title={t("figura.uninstall")}
                aria-label={t("figura.uninstall")}
                className="grid size-6 place-items-center rounded text-ink-muted outline-none hover:bg-surface-3 hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring"
              >
                <X className="size-3.5" />
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

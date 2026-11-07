import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowUpCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { showToast } from "@/components/toast";
import { errorText } from "@/lib/errors";
import type { Instance } from "@/features/instances/types";
import { modpackApi, type PackVersion } from "./api";
import { openChangeVersion } from "./version-dialog";

// Ответы на сессию: не дёргаем API при каждом открытии страницы сборки
const cache = new Map<string, PackVersion | null>();

/** Самая новая версия модпака, если она новее установленной. Бета и альфа — только если релизов нет вовсе. */
async function newer(instance: Instance): Promise<PackVersion | null> {
  const pack = instance.pack!;
  const list = await modpackApi.versions(pack.source, pack.projectId);
  const latest = list.find((v) => v.kind === "release") ?? list[0];
  return latest && latest.id !== pack.versionId ? latest : null;
}

/** Есть ли новая версия модпака; проверка тихо идёт при открытии сборки. */
export function usePackUpdate(instance: Instance | undefined) {
  const { t } = useTranslation();
  const key = `${instance?.id}/${instance?.pack?.versionId}`;
  const [update, setUpdate] = useState<PackVersion | null | undefined>(cache.get(key));
  const [checking, setChecking] = useState(false);

  const check = async (quiet: boolean) => {
    if (!instance?.pack) return;
    setChecking(true);
    try {
      const found = await newer(instance);
      cache.set(key, found);
      setUpdate(found);
      if (!found && !quiet) showToast(t("packUpdate.latest"));
    } catch (e) {
      if (!quiet) showToast(t("packUpdate.failed", { error: errorText(e) }));
    } finally {
      setChecking(false);
    }
  };

  useEffect(() => {
    setUpdate(cache.get(key));
    if (!cache.has(key)) void check(true);
  }, [key]);

  return { update, checking, check: () => check(false) };
}

/** Кнопка «Обновление: 1.3.0» в шапке — только когда обновление нашлось. */
export function PackUpdateButton({ instance, update, disabled }: { instance: Instance; update: PackVersion | null | undefined; disabled: boolean }) {
  const { t } = useTranslation();
  if (!update) return null;
  return (
    <Button variant="secondary" disabled={disabled} onClick={() => openChangeVersion(instance)} className="text-brand">
      <ArrowUpCircle /> <span className="max-w-[160px] truncate">{t("packUpdate.available", { name: update.name })}</span>
    </Button>
  );
}

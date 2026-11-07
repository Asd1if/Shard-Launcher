import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { FilePlus2, FolderOpen, Loader2, Shapes, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/page";
import { showToast } from "@/components/toast";
import { useInstances } from "@/features/instances/store";
import { InstallMenu, InstalledIn } from "@/features/skins/install-targets";
import { formatSize } from "@/lib/format";
import { isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { CPM_MOD, cpmApi, pickCpmModels, type CpmModel } from "./api";

/** Библиотека моделей CPM и установка в сборки. */
export function useCpm() {
  const { t } = useTranslation();
  const instances = useInstances();
  const [models, setModels] = useState<CpmModel[] | null>(null);
  const [installed, setInstalled] = useState<[string, string][]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const load = async () => {
    const [list, inst] = await Promise.all([cpmApi.list(), cpmApi.installed()]);
    setModels(list);
    setInstalled(inst);
    setSelected((s) => (s && list.some((m) => m.id === s) ? s : (list[0]?.id ?? null)));
  };
  useEffect(() => {
    void load().catch((e) => showToast(errorText(e)));
  }, []);

  const model = models?.find((m) => m.id === selected) ?? null;
  const installedIn = useMemo(() => (model ? installed.filter(([, f]) => f === model.id).map(([id]) => id) : []), [installed, model]);

  const run = async (key: string, fn: () => Promise<void>) => {
    setBusy(key);
    try {
      await fn();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  return {
    models,
    model,
    installed,
    installedIn,
    busy,
    select: setSelected,
    add: async () => {
      if (!isTauri) return showToast(t("figura.previewOnly"));
      const paths = await pickCpmModels(t("cpm.pick"));
      if (!paths.length) return;
      await run("add", async () => {
        let last = "";
        for (const p of paths) last = await cpmApi.add(p);
        await load();
        setSelected(last);
      });
    },
    install: (instanceId: string) =>
      model &&
      run("install", async () => {
        await cpmApi.install(model.id, instanceId);
        setInstalled(await cpmApi.installed());
        showToast(t("figura.installed", { name: model.name, instance: instances.find((i) => i.id === instanceId)?.name }));
      }),
    uninstall: (instanceId: string) =>
      model &&
      run("uninstall", async () => {
        await cpmApi.uninstall(model.id, instanceId);
        setInstalled(await cpmApi.installed());
      }),
    remove: () =>
      model &&
      run("remove", async () => {
        await cpmApi.remove(model.id);
        await load();
      }),
  };
}

export type Cpm = ReturnType<typeof useCpm>;

export function CpmLibrary({ cpm }: { cpm: Cpm }) {
  const { t } = useTranslation();
  const { models, model, installed, busy } = cpm;

  if (models === null) {
    return (
      <p className="flex items-center gap-2 py-6 text-[13px] text-ink-muted">
        <Loader2 className="size-4 animate-spin" /> {t("worlds.loading")}
      </p>
    );
  }

  const addButton = (
    <Button size="sm" disabled={busy === "add"} onClick={() => void cpm.add()}>
      {busy === "add" ? <Loader2 className="animate-spin" /> : <FilePlus2 />} {t("cpm.add")}
    </Button>
  );

  if (models.length === 0) {
    return <EmptyState icon={<Shapes className="size-7" />} title={t("cpm.emptyTitle")} text={t("cpm.emptyText")} action={addButton} />;
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <p className="min-w-0 flex-1 text-[13px] text-ink-muted">{t("cpm.libraryHint")}</p>
        <Button size="sm" variant="ghost" onClick={() => void cpmApi.open()}>
          <FolderOpen /> {t("cpm.openFolder")}
        </Button>
        {addButton}
      </div>
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-2">
        {models.map((m) => {
          const count = installed.filter(([, f]) => f === m.id).length;
          return (
            <li key={m.id}>
              <button
                type="button"
                onClick={() => cpm.select(m.id)}
                aria-pressed={m.id === model?.id}
                className={cn(
                  "flex w-full flex-col items-center gap-2 rounded-lg border bg-surface-1 p-3 outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
                  m.id === model?.id ? "border-brand" : "border-surface-3 hover:border-ink-muted",
                )}
              >
                <span className="pixel-corners grid size-14 place-items-center bg-surface-3 text-ink-muted [--step:3px]">
                  <Shapes className="size-6" />
                </span>
                <span className="w-full truncate text-center text-[13px]">{m.name}</span>
                <span className="w-full truncate text-center text-[11px] text-ink-muted">
                  {count ? t("figura.inInstances", { count }) : t("figura.notInstalled")}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export function CpmDetails({ cpm }: { cpm: Cpm }) {
  const { t, i18n } = useTranslation();
  const { model, busy } = cpm;
  if (!model) return null;
  return (
    <>
      <div className="min-w-0">
        <p className="truncate font-display text-[15px]">{model.name}</p>
        <p className="truncate text-xs text-ink-muted">
          {model.id} · {formatSize(model.size, i18n.language)}
        </p>
      </div>
      <div className="flex gap-2">
        <InstallMenu installedIn={cpm.installedIn} busy={busy === "install"} onInstall={(id) => void cpm.install(id)} />
        <Button size="icon" variant="ghost" title={t("figura.remove")} aria-label={t("figura.remove")} disabled={busy !== null} onClick={() => void cpm.remove()} className="hover:text-destructive">
          <Trash2 />
        </Button>
      </div>
      <InstalledIn ids={cpm.installedIn} mod={CPM_MOD} modPattern={/customizable player models|cpm|custom.?player.?models/i} onUninstall={(id) => void cpm.uninstall(id)} />
    </>
  );
}

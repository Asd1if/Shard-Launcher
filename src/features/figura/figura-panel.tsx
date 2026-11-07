import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Box, FileArchive, FolderOpen, FolderPlus, Loader2, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/page";
import { showToast } from "@/components/toast";
import { useInstances } from "@/features/instances/store";
import { InstallMenu, InstalledIn } from "@/features/skins/install-targets";
import { formatSize } from "@/lib/format";
import { isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { figuraApi, pickAvatar, type Avatar } from "./api";
import type { BbModel } from "./bbmodel";

export interface FiguraFiles {
  models: BbModel[];
  textures: Map<string, string>;
}

/** Библиотека моделей Figura: список, выбранный аватар, его файлы для превью и установка в сборки. */
export function useFigura() {
  const { t } = useTranslation();
  const instances = useInstances();
  const [avatars, setAvatars] = useState<Avatar[] | null>(null);
  const [installed, setInstalled] = useState<[string, string][]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [files, setFiles] = useState<FiguraFiles | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const load = async () => {
    const [list, inst] = await Promise.all([figuraApi.list(), figuraApi.installed()]);
    setAvatars(list);
    setInstalled(inst);
    setSelected((s) => (s && list.some((a) => a.id === s) ? s : (list[0]?.id ?? null)));
  };
  useEffect(() => {
    void load().catch((e) => showToast(errorText(e)));
  }, []);

  const avatar = avatars?.find((a) => a.id === selected) ?? null;

  useEffect(() => {
    setFiles(null);
    if (!avatar) return;
    let cancelled = false;
    figuraApi
      .files(avatar.id)
      .then((f) => {
        if (cancelled) return;
        const models: BbModel[] = [];
        for (const [, text] of f.models) {
          try {
            models.push(JSON.parse(text));
          } catch {
            // битая модель — просто не показываем её
          }
        }
        setFiles({ models, textures: new Map(f.textures.map(([path, data]) => [path.split("/").pop()!, data])) });
      })
      .catch((e) => showToast(errorText(e)));
    return () => {
      cancelled = true;
    };
  }, [avatar?.id]);

  const installedIn = useMemo(
    () => (avatar ? installed.filter(([, folder]) => folder === avatar.folder).map(([id]) => id) : []),
    [installed, avatar],
  );

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
    avatars,
    avatar,
    files,
    installed,
    installedIn,
    busy,
    select: setSelected,
    add: async (folder: boolean) => {
      if (!isTauri) return showToast(t("figura.previewOnly"));
      const path = await pickAvatar(t("figura.pick"), folder);
      if (!path) return;
      await run("add", async () => {
        const a = await figuraApi.add(path);
        await load();
        setSelected(a.id);
        showToast(t("figura.added", { name: a.name }));
      });
    },
    install: (instanceId: string) =>
      avatar &&
      run("install", async () => {
        await figuraApi.install(avatar.id, instanceId);
        setInstalled(await figuraApi.installed());
        showToast(t("figura.installed", { name: avatar.name, instance: instances.find((i) => i.id === instanceId)?.name }));
      }),
    uninstall: (instanceId: string) =>
      avatar &&
      run("uninstall", async () => {
        await figuraApi.uninstall(avatar.id, instanceId);
        setInstalled(await figuraApi.installed());
      }),
    remove: () =>
      avatar &&
      run("remove", async () => {
        await figuraApi.remove(avatar.id);
        await load();
      }),
  };
}

export type Figura = ReturnType<typeof useFigura>;

/** Кнопки добавления и сетка аватаров. */
export function FiguraLibrary({ figura }: { figura: Figura }) {
  const { t } = useTranslation();
  const { avatars, avatar, installed, busy } = figura;

  if (avatars === null) {
    return (
      <p className="flex items-center gap-2 py-6 text-[13px] text-ink-muted">
        <Loader2 className="size-4 animate-spin" /> {t("worlds.loading")}
      </p>
    );
  }

  const addButtons = (
    <div className="flex gap-2">
      <Button size="sm" disabled={busy === "add"} onClick={() => void figura.add(false)}>
        {busy === "add" ? <Loader2 className="animate-spin" /> : <FileArchive />} {t("figura.addZip")}
      </Button>
      <Button size="sm" variant="secondary" disabled={busy === "add"} onClick={() => void figura.add(true)}>
        <FolderPlus /> {t("figura.addFolder")}
      </Button>
    </div>
  );

  if (avatars.length === 0) {
    return <EmptyState icon={<Box className="size-7" />} title={t("figura.emptyTitle")} text={t("figura.emptyText")} action={addButtons} />;
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <p className="min-w-0 flex-1 text-[13px] text-ink-muted">{t("figura.libraryHint")}</p>
        {addButtons}
      </div>
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-2">
        {avatars.map((a) => {
          const count = installed.filter(([, f]) => f === a.folder).length;
          return (
            <li key={a.id}>
              <button
                type="button"
                onClick={() => figura.select(a.id)}
                aria-pressed={a.id === avatar?.id}
                className={cn(
                  "flex w-full flex-col items-center gap-2 rounded-lg border bg-surface-1 p-3 outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
                  a.id === avatar?.id ? "border-brand" : "border-surface-3 hover:border-ink-muted",
                )}
              >
                <span
                  className="pixel-corners grid size-14 place-items-center font-display text-xl font-bold text-white [--step:3px]"
                  style={{ backgroundColor: a.color ?? "#5a5a66" }}
                >
                  {a.name.slice(0, 1).toUpperCase()}
                </span>
                <span className="w-full truncate text-center text-[13px]">{a.name}</span>
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

/** Под превью: сведения о выбранном аватаре и установка в сборки. */
export function FiguraDetails({ figura }: { figura: Figura }) {
  const { t, i18n } = useTranslation();
  const { avatar, busy } = figura;
  if (!avatar) return null;
  return (
    <>
      <div className="min-w-0">
        <p className="truncate font-display text-[15px]">{avatar.name}</p>
        <p className="truncate text-xs text-ink-muted">
          {[
            avatar.authors.length ? t("content.by", { author: avatar.authors.join(", ") }) : null,
            t("figura.models", { count: avatar.models.length }),
            avatar.scripts ? t("figura.scripts", { count: avatar.scripts }) : null,
            formatSize(avatar.size, i18n.language),
          ]
            .filter(Boolean)
            .join(" · ")}
        </p>
        {avatar.description && <p className="mt-1 line-clamp-2 text-xs text-ink-muted">{avatar.description}</p>}
      </div>
      <div className="flex gap-2">
        <InstallMenu installedIn={figura.installedIn} busy={busy === "install"} onInstall={(id) => void figura.install(id)} />
        <Button size="icon" variant="ghost" title={t("figura.openFolder")} aria-label={t("figura.openFolder")} onClick={() => void figuraApi.open(avatar.id)}>
          <FolderOpen />
        </Button>
        <Button size="icon" variant="ghost" title={t("figura.remove")} aria-label={t("figura.remove")} disabled={busy !== null} onClick={() => void figura.remove()} className="hover:text-destructive">
          <Trash2 />
        </Button>
      </div>
      <InstalledIn ids={figura.installedIn} mod="figura" modPattern={/figura/i} onUninstall={(id) => void figura.uninstall(id)} />
    </>
  );
}

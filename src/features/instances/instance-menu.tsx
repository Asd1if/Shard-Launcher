import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import {
  Copy,
  FolderOpen,
  FolderTree,
  History,
  ImageIcon,
  PackageOpen,
  Play,
  Server,
  Settings2,
  Square,
  Star,
  Trash2,
} from "lucide-react";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { showToast } from "@/components/toast";
import { launch, stop, useLaunch } from "@/features/launch/store";
import { openChangeVersion } from "@/features/modpack/version-dialog";
import { backend } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { openInstanceDialog } from "./instance-dialogs";
import { setInstanceMeta } from "./store";
import type { Instance } from "./types";

/** Меню по правому клику на сборке: запуск, папка, копия, экспорт, группа, удаление. */
export function InstanceContextMenu({ instance, children }: { instance: Instance; children: ReactNode }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const state = useLaunch(instance.id);
  const busy = state.status !== "idle";
  const server = instance.settings?.server;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent>
        {state.status === "running" ? (
          <ContextMenuItem onSelect={() => void stop(instance.id)}>
            <Square /> {t("launch.stop")}
          </ContextMenuItem>
        ) : (
          <ContextMenuItem disabled={busy} onSelect={() => void launch(instance.id, instance.name)}>
            <Play /> {t("launch.play")}
          </ContextMenuItem>
        )}
        {server && (
          <ContextMenuItem disabled={busy} onSelect={() => void launch(instance.id, instance.name, true)}>
            <Server /> {t("server.join", { server })}
          </ContextMenuItem>
        )}
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => void setInstanceMeta(instance.id, instance.group ?? null, !instance.favorite)}>
          <Star className={instance.favorite ? "fill-current text-[#e0b040]" : undefined} />
          {instance.favorite ? t("groups.unfavorite") : t("groups.favorite")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => openInstanceDialog("group", instance.id)}>
          <FolderTree /> {t("groups.move")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => openInstanceDialog("icon", instance.id)}>
          <ImageIcon /> {t("icon.change")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => void backend.openInstanceDir(instance.id).catch((e) => showToast(errorText(e)))}>
          <FolderOpen /> {t("instance.openFolder")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => navigate(`/instances/${instance.id}?tab=settings`)}>
          <Settings2 /> {t("instance.tab.settings")}
        </ContextMenuItem>
        {instance.pack && (
          <ContextMenuItem disabled={busy} onSelect={() => openChangeVersion(instance)}>
            <History /> {t("packVersion.menu")}
          </ContextMenuItem>
        )}
        <ContextMenuItem onSelect={() => openInstanceDialog("clone", instance.id)}>
          <Copy /> {t("clone.menu")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => openInstanceDialog("export", instance.id)}>
          <PackageOpen /> {t("instance.export")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem variant="destructive" disabled={busy} onSelect={() => openInstanceDialog("delete", instance.id)}>
          <Trash2 /> {t("instance.delete")}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

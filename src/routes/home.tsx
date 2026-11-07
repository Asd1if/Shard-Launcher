import { useState } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { Boxes, Download, Play, Plus, SlidersHorizontal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { EmptyState, Page } from "@/components/page";
import { PlayButton } from "@/components/play-button";
import { InstanceIcon } from "@/features/instances/instance-icon";
import {
  useInstanceDetails,
  useInstanceLabel,
  VersionTypeBadge,
} from "@/features/instances/instance-meta";
import { byLastPlayed, useInstances } from "@/features/instances/store";
import { CreateInstanceDialog } from "@/features/instances/create-instance-dialog";
import { openImport } from "@/features/modpack/import-dialog";
import { timeAgo } from "@/lib/time";
import { launch } from "@/features/launch/store";
import { InstanceContextMenu } from "@/features/instances/instance-menu";
import { MinecraftNews } from "@/features/news/news";

export function HomePage() {
  const { t, i18n } = useTranslation();
  const label = useInstanceLabel();
  const details = useInstanceDetails();
  const [creating, setCreating] = useState(false);
  const [newsVersion, setNewsVersion] = useState<string | null>(null);
  const createFrom = (version: string) => {
    setNewsVersion(version);
    setCreating(true);
  };
  const dialog = (
    <CreateInstanceDialog
      open={creating}
      initialVersion={newsVersion}
      onOpenChange={(o) => {
        setCreating(o);
        if (!o) setNewsVersion(null);
      }}
    />
  );
  const [current, ...others] = useInstances().toSorted(byLastPlayed);

  if (!current) {
    return (
      <Page>
        <EmptyState
          icon={<Boxes className="size-8" />}
          title={t("home.emptyTitle")}
          text={t("home.emptyText")}
          action={
            <div className="flex gap-2">
              <Button onClick={() => setCreating(true)}>
                <Plus /> {t("home.create")}
              </Button>
              <Button variant="secondary" onClick={() => openImport()}>
                <Download /> {t("instances.import")}
              </Button>
            </div>
          }
        />
        <MinecraftNews onCreate={createFrom} />
        {dialog}
      </Page>
    );
  }

  return (
    <div className="relative min-h-full">
      <Page className="relative">
        <InstanceContextMenu instance={current}>
          <section className="relative overflow-hidden rounded-xl border border-surface-3 bg-surface-1/80 backdrop-blur-md">
            {/* Крупный узор сборки уходит за край — фон-«обложка» без картинок */}
            <InstanceIcon
              name={current.name}
              loader={current.loader}
              icon={current.icon}
              className="absolute -top-12 -right-12 size-[320px] opacity-25 [--step:12px] [mask-image:radial-gradient(circle_at_70%_30%,black,transparent_70%)]"
            />
            <div className="relative flex flex-col gap-7 p-9">
              <div className="flex items-center gap-4">
                <InstanceIcon
                  name={current.name}
                  loader={current.loader}
                  icon={current.icon}
                  className="size-14 [--step:3px]"
                />
                <div className="min-w-0">
                  <h1 className="font-display text-[28px] leading-tight font-medium tracking-tight">
                    {current.name}
                  </h1>
                  <p className="flex items-center gap-2 text-ink-muted">
                    {details(current)}
                    {current.modCount > 0 &&
                      `, ${t("instances.mods", { count: current.modCount })}`}
                    <VersionTypeBadge type={current.versionType} />
                  </p>
                </div>
              </div>
              <div className="flex flex-wrap items-center gap-4">
                <PlayButton
                  instanceId={current.id}
                  instanceName={current.name}
                />
                <Button variant="ghost" asChild className="text-ink-muted">
                  <Link to={`/instances/${current.id}`}>
                    <SlidersHorizontal /> {t("home.settings")}
                  </Link>
                </Button>
                <span className="ml-auto text-sm text-ink-muted">
                  {current.lastPlayed
                    ? t("home.lastPlayed", {
                        when: timeAgo(current.lastPlayed, i18n.language),
                      })
                    : t("home.neverPlayed")}
                </span>
              </div>
            </div>
          </section>
        </InstanceContextMenu>

        {others.length > 0 && (
          <section className="flex flex-col gap-3">
            <h2 className="text-sm text-ink-muted">
              {t("home.otherInstances")}
            </h2>
            <ul className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-2">
              {others.map((inst) => (
                <InstanceContextMenu key={inst.id} instance={inst}>
                  <li className="group flex items-center gap-3 rounded-lg border border-transparent bg-surface-1/85 p-3 backdrop-blur-md transition-colors hover:border-surface-3">
                    <InstanceIcon
                      name={inst.name}
                      loader={inst.loader}
                      icon={inst.icon}
                      className="size-10"
                    />
                    <Link
                      to={`/instances/${inst.id}`}
                      className="min-w-0 flex-1 outline-none focus-visible:underline"
                    >
                      <span className="block truncate text-foreground">
                        {inst.name}
                      </span>
                      <span className="flex items-center gap-1.5 text-xs text-ink-muted">
                        <span className="truncate">{label(inst)}</span>
                        <VersionTypeBadge type={inst.versionType} />
                      </span>
                    </Link>
                    <button
                      type="button"
                      onClick={() => void launch(inst.id, inst.name)}
                      aria-label={`${t("home.play")}: ${inst.name}`}
                      title={t("home.play")}
                      className="pixel-corners grid size-8 shrink-0 place-items-center bg-surface-2 text-ink-muted outline-none transition-colors [--step:2px] hover:bg-brand hover:text-brand-foreground focus-visible:bg-brand focus-visible:text-brand-foreground"
                    >
                      <Play className="size-3.5 fill-current" />
                    </button>
                  </li>
                </InstanceContextMenu>
              ))}
            </ul>
          </section>
        )}
        <MinecraftNews onCreate={createFrom} />
        {dialog}
      </Page>
    </div>
  );
}

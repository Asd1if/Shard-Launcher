import { useTranslation } from "react-i18next";
import { Download, FileArchive, ListTree } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Page } from "@/components/page";
import { ProjectSearch } from "@/components/project-search";
import { openImport } from "@/features/modpack/import-dialog";
import { modpackApi } from "@/features/modpack/api";
import { openPackVersions } from "@/features/modpack/version-dialog";

/** Готовые сборки с Modrinth и CurseForge: установка создаёт новую сборку через импорт. */
export function ModpacksPage() {
  const { t } = useTranslation();

  return (
    <Page
      title={t("modpacks.title")}
      actions={
        <Button variant="ghost" onClick={() => openImport()}>
          <FileArchive /> {t("modpacks.fromFile")}
        </Button>
      }
    >
      <ProjectSearch
        resetKey="modpacks"
        projectType="modpack"
        filters={{ loaders: ["fabric", "forge", "neoforge", "quilt"] }}
        search={(q) => modpackApi.search(q)}
        placeholder={(source) => t("modpacks.searchPlaceholder", { source })}
        toolbar={<span className="flex-1" />}
        action={(hit) => (
          <div className="mt-1 flex shrink-0 gap-1">
            <Button
              size="icon"
              variant="ghost"
              title={t("packVersion.choose")}
              aria-label={t("packVersion.choose")}
              onClick={() =>
                openPackVersions({
                  mode: "install",
                  source: hit.source,
                  projectId: hit.projectId,
                  name: hit.title,
                  url: hit.source === "modrinth" ? (hit.pageUrl ?? `https://modrinth.com/modpack/${hit.slug}`) : undefined,
                })
              }
            >
              <ListTree />
            </Button>
            <Button
              size="sm"
              variant="secondary"
              className="w-[124px]"
              onClick={() =>
                openImport(
                  hit.source === "curseforge"
                    ? { curseforgeId: Number(hit.projectId), name: hit.title }
                    : { url: hit.pageUrl ?? `https://modrinth.com/modpack/${hit.slug}`, name: hit.title },
                )
              }
            >
              <Download /> {t("content.install")}
            </Button>
          </div>
        )}
      />
    </Page>
  );
}

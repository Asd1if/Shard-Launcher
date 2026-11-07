import { useTranslation } from "react-i18next";
import { CheckUpdatesButton } from "@/features/updater/updater";
import { Check } from "lucide-react";
import { Page } from "@/components/page";
import { languages, setLanguage, type Language } from "@/i18n";
import { useState } from "react";
import { extraThemes, mainThemes, setTheme, themeModes, useThemeChoice, type Theme, type ThemeChoice } from "@/theme";
import { cn } from "@/lib/utils";
import { ScrollText } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { TermsText } from "@/features/onboarding/terms";
import { BackdropSettings, LauncherSettings } from "@/features/settings/launcher-settings";

// Превью каждой темы рисуется её собственными цветами, независимо от активной.
type Preview = { bg: string; panel: string; line: string; brand: string };
const themePreview: Record<Theme, Preview> = {
  graphite: { bg: "#141416", panel: "#1b1b1e", line: "#303035", brand: "#e4e4e7" },
  amethyst: { bg: "#000000", panel: "#0c0a10", line: "#221e2b", brand: "#b28cff" },
  light: { bg: "#f3f3f5", panel: "#ffffff", line: "#d8d8de", brand: "#26262b" },
  emerald: { bg: "#0d1210", panel: "#121a16", line: "#24332c", brand: "#4ade80" },
  amber: { bg: "#14120c", panel: "#1b1810", line: "#342e20", brand: "#facc15" },
  ocean: { bg: "#0b1016", panel: "#10171f", line: "#22303e", brand: "#38bdf8" },
  rose: { bg: "#140d10", panel: "#1b1216", line: "#36232b", brand: "#fb7185" },
};

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const theme = useThemeChoice();
  const [group, setGroup] = useState<"main" | "extra">((extraThemes as readonly string[]).includes(theme) ? "extra" : "main");

  return (
    <Page title={t("settings.title")}>
      <section className="flex flex-col gap-3">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <h2 className="text-foreground">{t("settings.theme")}</h2>
            <p className="text-sm text-ink-muted">{t("settings.themeHint")}</p>
          </div>
          <div role="tablist" className="flex rounded-md bg-surface-2 p-0.5">
            {(["main", "extra"] as const).map((g) => (
              <button
                key={g}
                type="button"
                role="tab"
                aria-selected={group === g}
                onClick={() => setGroup(g)}
                className={cn(
                  "h-7 rounded px-3 text-xs text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                  group === g && "bg-surface-0 text-foreground",
                )}
              >
                {t(`settings.themeGroups.${g}`)}
              </button>
            ))}
          </div>
        </div>
        <div role="radiogroup" aria-label={t("settings.theme")} className="grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-3">
          {(group === "main" ? [...themeModes, ...mainThemes] : extraThemes).map((key: ThemeChoice) => {
            // «Системная» и «Авто» — пополам тёмная и светлая
            const halves = key === "system" || key === "auto" ? [themePreview.graphite, themePreview.light] : [themePreview[key]];
            const active = theme === key;
            return (
              <button
                key={key}
                type="button"
                role="radio"
                aria-checked={active}
                onClick={() => setTheme(key)}
                className={cn(
                  "flex flex-col gap-2.5 rounded-lg border bg-surface-1 p-2.5 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
                  active ? "border-ink-muted" : "border-surface-3 hover:border-ink-muted/50",
                )}
              >
                <span aria-hidden className="flex h-20 overflow-hidden rounded-md border" style={{ borderColor: halves[0].line }}>
                  {halves.map((p, i) => (
                    <span key={i} className="flex flex-1" style={{ backgroundColor: p.bg }}>
                      {i === 0 && <span className="w-1/4 border-r" style={{ backgroundColor: p.panel, borderColor: p.line }} />}
                      <span className="flex flex-1 flex-col justify-end gap-1.5 p-2.5">
                        <span className="h-1.5 w-2/3 rounded-full" style={{ backgroundColor: p.line }} />
                        <span className="pixel-corners h-4 w-12 [--step:1px]" style={{ backgroundColor: p.brand }} />
                      </span>
                    </span>
                  ))}
                </span>
                <span className="flex items-center justify-between px-0.5 text-[13px]">
                  {t(`settings.themes.${key}`)}
                  {active && <Check className="size-4 text-foreground" />}
                </span>
              </button>
            );
          })}
        </div>
      </section>

      <BackdropSettings />
      <LauncherSettings />

      <section className="flex flex-col divide-y divide-surface-3 rounded-lg border border-surface-3 bg-surface-1">
        <div className="flex items-center justify-between gap-6 px-5 py-4">
          <div>
            <h2 className="text-foreground">{t("settings.language")}</h2>
            <p className="text-sm text-ink-muted">{t("settings.languageHint")}</p>
          </div>
          <div role="radiogroup" aria-label={t("settings.language")} className="flex rounded-md bg-surface-2 p-1">
            {languages.map((lang) => (
              <button
                key={lang.code}
                type="button"
                role="radio"
                aria-checked={i18n.resolvedLanguage === lang.code}
                onClick={() => setLanguage(lang.code as Language)}
                className={cn(
                  "h-7 rounded px-3 text-[13px] text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                  i18n.resolvedLanguage === lang.code && "bg-foreground font-medium text-background hover:text-background",
                )}
              >
                {lang.label}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center justify-between gap-6 px-5 py-4">
          <h2 className="text-foreground">{t("settings.about")}</h2>
          <div className="flex items-center gap-3">
            <Dialog>
              <DialogTrigger asChild>
                <Button variant="ghost" size="sm">
                  <ScrollText /> {t("terms.title")}
                </Button>
              </DialogTrigger>
              <DialogContent className="border-surface-3 bg-surface-1 sm:max-w-[560px]">
                <DialogHeader>
                  <DialogTitle>{t("terms.title")}</DialogTitle>
                </DialogHeader>
                <div className="max-h-[60vh] overflow-y-auto pr-1">
                  <TermsText />
                </div>
              </DialogContent>
            </Dialog>
            <div className="flex items-center gap-3">
              <p className="text-sm text-ink-muted">{t("settings.version", { version: __APP_VERSION__ })}</p>
              <CheckUpdatesButton />
            </div>
          </div>
        </div>
      </section>
    </Page>
  );
}

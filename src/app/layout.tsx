import { Outlet } from "react-router";
import { Titlebar } from "@/components/titlebar";
import { Sidebar } from "@/components/sidebar";
import { Toaster } from "@/components/toast";
import { AccountsDialog } from "@/features/accounts/accounts-dialog";
import { ManualDownloadDialog } from "@/features/content/manual-download-dialog";
import { ImportDialog } from "@/features/modpack/import-dialog";
import { PackVersionDialog } from "@/features/modpack/version-dialog";
import { InstanceDialogs } from "@/features/instances/instance-dialogs";
import { CrashDialog } from "@/features/launch/crash-dialog";
import { useDiscordPresence } from "@/features/discord/presence";
import { Backdrop, useBackdrop, useBackdropTone } from "@/features/backdrop/backdrop";
import { useTheme } from "@/theme";
import { Onboarding, useOnboarding } from "@/features/onboarding/onboarding";

export function AppLayout() {
  useDiscordPresence();
  const backdrop = useBackdrop();
  useBackdropTone();
  // Холсты фона берут цвета темы при запуске — при смене темы рисуем заново
  const theme = useTheme();
  const onboarding = useOnboarding();

  if (onboarding.loading || onboarding.show) {
    return (
      <div className="relative isolate flex h-full flex-col bg-background">
        <Backdrop key={theme} {...backdrop} className="-z-10" />
        <Titlebar />
        {onboarding.show && onboarding.settings && <Onboarding initial={onboarding.settings} onDone={onboarding.finish} />}
        <AccountsDialog />
        <Toaster />
      </div>
    );
  }

  return (
    <div className="relative isolate flex h-full flex-col bg-background">
      {/* Фон один на всё окно: за боковым меню и любой страницей */}
      <Backdrop key={theme} {...backdrop} className="-z-10" />
      <Titlebar />
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="min-w-0 flex-1 overflow-y-auto">
          <Outlet />
        </main>
      </div>
      <AccountsDialog />
      <ImportDialog />
      <ManualDownloadDialog />
      <PackVersionDialog />
      <InstanceDialogs />
      <CrashDialog />
      <Toaster />
    </div>
  );
}

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { ShardLogo } from "@/brand/shard-logo";
import { UpdateChip } from "@/features/updater/updater";

const isTauri = "__TAURI_INTERNALS__" in window;

export function Titlebar() {
  const { t } = useTranslation();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri) return;
    const win = getCurrentWindow();
    void win.isMaximized().then(setMaximized);
    const unlisten = win.onResized(() => void win.isMaximized().then(setMaximized));
    return () => void unlisten.then((fn) => fn());
  }, []);

  const win = isTauri ? getCurrentWindow() : null;

  return (
    <header
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center border-b border-sidebar-border bg-sidebar/45 backdrop-blur-md"
    >
      <div data-tauri-drag-region className="flex items-center gap-2 pl-4">
        <ShardLogo className="pointer-events-none h-4 w-auto" />
        <span data-tauri-drag-region className="font-display text-[11px] font-medium text-ink-muted tracking-wide">
          Shard Launcher
        </span>
      </div>
      <div data-tauri-drag-region className="flex-1 self-stretch" />
      <UpdateChip />
      <div className="flex self-stretch">
        <WindowButton label={t("window.minimize")} onClick={() => void win?.minimize()}>
          <Minus className="size-4" />
        </WindowButton>
        <WindowButton label={t("window.maximize")} onClick={() => void win?.toggleMaximize()}>
          {maximized ? <Copy className="size-3.5 -scale-x-100" /> : <Square className="size-3.5" />}
        </WindowButton>
        <WindowButton label={t("window.close")} onClick={() => void win?.close()} danger>
          <X className="size-4" />
        </WindowButton>
      </div>
    </header>
  );
}

function WindowButton({
  label,
  onClick,
  danger,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        "grid w-12 place-items-center text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:bg-sidebar-accent",
        danger ? "hover:bg-[#c42b3c] hover:text-white" : "hover:bg-sidebar-accent",
      )}
    >
      {children}
    </button>
  );
}

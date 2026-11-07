import { NavLink } from "react-router";
import { useTranslation } from "react-i18next";
import { Boxes, ChevronsUpDown, House, LogIn, Package, Server, Settings, Shirt, type LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { InstanceIcon } from "@/features/instances/instance-icon";
import { useInstances } from "@/features/instances/store";
import { useInstanceLabel } from "@/features/instances/instance-meta";
import { currentAccount, openAccountsDialog, useAccounts } from "@/features/accounts/store";
import { PlayerHead } from "@/features/accounts/player-head";
import { useAccountFace } from "@/features/accounts/account-face";
import { LocatorDot } from "@/features/accounts/locator-color";

const nav: { to: string; key: string; icon: LucideIcon }[] = [
  { to: "/", key: "nav.home", icon: House },
  { to: "/instances", key: "nav.instances", icon: Boxes },
  { to: "/modpacks", key: "nav.modpacks", icon: Package },
  { to: "/servers", key: "nav.servers", icon: Server },
  { to: "/skins", key: "nav.skins", icon: Shirt },
];

export function Sidebar() {
  const { t } = useTranslation();
  const label = useInstanceLabel();
  const account = currentAccount(useAccounts());
  const face = useAccountFace(account);
  const recent = useInstances()
    .filter((i) => i.lastPlayed)
    .toSorted((a, b) => b.lastPlayed!.localeCompare(a.lastPlayed!))
    .slice(0, 4);

  return (
    <aside className="flex w-[236px] shrink-0 flex-col border-r border-sidebar-border bg-sidebar/45 text-sidebar-foreground backdrop-blur-md">
      <nav className="flex flex-col gap-0.5 p-3">
        {nav.map((item) => (
          <SidebarLink key={item.to} to={item.to} icon={item.icon} end={item.to === "/"}>
            {t(item.key)}
          </SidebarLink>
        ))}
      </nav>

      <div className="mx-3 border-t border-sidebar-border" />

      <section className="flex min-h-0 flex-1 flex-col p-3">
        <h2 className="px-2 pb-2 text-xs text-ink-muted">{t("sidebar.recent")}</h2>
        {recent.length === 0 ? (
          <p className="px-2 text-xs leading-relaxed text-ink-muted">{t("sidebar.noRecent")}</p>
        ) : (
          <ul className="flex flex-col gap-0.5 overflow-y-auto">
            {recent.map((inst) => (
              <li key={inst.id}>
                <NavLink
                  to={`/instances/${inst.id}`}
                  className={({ isActive }) =>
                    cn(
                      "flex items-center gap-2.5 rounded-md px-2 py-1.5 outline-none transition-colors hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-ring",
                      isActive && "bg-sidebar-accent text-sidebar-accent-foreground",
                    )
                  }
                >
                  <InstanceIcon name={inst.name} loader={inst.loader} icon={inst.icon} className="size-7" />
                  <span className="min-w-0">
                    <span className="block truncate text-[13px] text-foreground">{inst.name}</span>
                    <span className="block truncate text-[11px] text-ink-muted">
                      {label(inst)}
                    </span>
                  </span>
                </NavLink>
              </li>
            ))}
          </ul>
        )}
      </section>

      <div className="border-t border-sidebar-border p-3">
        <SidebarLink to="/settings" icon={Settings}>
          {t("nav.settings")}
        </SidebarLink>
        <button
          type="button"
          onClick={openAccountsDialog}
          className="group mt-2 flex w-full items-center gap-2.5 rounded-md bg-surface-2 p-2 text-left outline-none transition-colors hover:bg-surface-3 focus-visible:ring-2 focus-visible:ring-ring"
        >
          {account ? (
            <>
              <PlayerHead name={account.name} face={face} className="size-8" />
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-1.5 text-[13px] text-foreground">
                  <LocatorDot uuid={account.uuid} />
                  <span className="truncate">{account.name}</span>
                </span>
                <span className="block truncate text-[11px] text-ink-muted">{t(`accounts.kind.${account.kind}`)}</span>
              </span>
              <ChevronsUpDown className="size-4 text-ink-muted" />
            </>
          ) : (
            <>
              <span className="pixel-corners grid size-8 shrink-0 place-items-center bg-surface-3 text-ink-muted [--step:2px] group-hover:bg-brand group-hover:text-brand-foreground">
                <LogIn className="size-4" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13px] text-foreground">{t("sidebar.signIn")}</span>
                <span className="block truncate text-[11px] text-ink-muted">{t("sidebar.signInHint")}</span>
              </span>
            </>
          )}
        </button>
      </div>
    </aside>
  );
}

function SidebarLink({
  to,
  icon: Icon,
  end,
  children,
}: {
  to: string;
  icon: LucideIcon;
  end?: boolean;
  children: React.ReactNode;
}) {
  return (
    <NavLink
      to={to}
      end={end}
      className={({ isActive }) =>
        cn(
          "group relative flex h-9 items-center gap-3 rounded-md px-2.5 text-[13.5px] outline-none transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-ring",
          isActive && "bg-sidebar-accent text-sidebar-accent-foreground",
        )
      }
    >
      {({ isActive }) => (
        <>
          <span
            aria-hidden
            className={cn(
              "absolute top-2 bottom-2 -left-3 w-[3px] rounded-r bg-brand transition-opacity",
              isActive ? "opacity-100" : "opacity-0",
            )}
          />
          <Icon className={cn("size-[18px]", isActive ? "text-brand" : "text-ink-muted group-hover:text-foreground")} />
          {children}
        </>
      )}
    </NavLink>
  );
}

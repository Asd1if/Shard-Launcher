import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import type { Account } from "@/lib/backend";
import { PlayerHead } from "./player-head";
import { useAccountFace } from "./account-face";
import { LocatorDot } from "./locator-color";
import { MicrosoftLogin } from "./microsoft-login";
import {
  addOfflineAccount,
  removeAccount,
  selectAccount,
  setAccountsDialogOpen,
  useAccounts,
  useAccountsDialog,
} from "./store";

const NICK = /^[A-Za-z0-9_]{3,16}$/;

export function AccountsDialog() {
  const { t } = useTranslation();
  const open = useAccountsDialog();
  const { accounts, selected } = useAccounts();
  const [nick, setNick] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const name = nick.trim();
    if (!NICK.test(name)) {
      setError(t("accounts.nickRules"));
      return;
    }
    try {
      await addOfflineAccount(name);
      setNick("");
      setError(null);
      setAccountsDialogOpen(false);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <Dialog open={open} onOpenChange={setAccountsDialogOpen}>
      <DialogContent className="gap-5 border-surface-3 bg-surface-1 sm:max-w-[440px]">
        <DialogHeader>
          <DialogTitle className="font-display text-lg font-medium">{t("accounts.title")}</DialogTitle>
          <DialogDescription>{t("accounts.description")}</DialogDescription>
        </DialogHeader>

        {accounts.length > 0 && (
          <ul role="radiogroup" aria-label={t("accounts.title")} className="flex flex-col gap-1">
            {accounts.map((account) => {
              const active = account.id === selected;
              return (
                <li key={account.id} className="group flex items-center gap-1">
                  <button
                    type="button"
                    role="radio"
                    aria-checked={active}
                    onClick={() => void selectAccount(account.id)}
                    className={cn(
                      "flex flex-1 items-center gap-3 rounded-md px-2.5 py-2 text-left outline-none transition-colors hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-ring",
                      active && "bg-surface-2",
                    )}
                  >
                    <AccountHead account={account} />
                    <span className="min-w-0 flex-1">
                      <span className="flex items-center gap-2 text-foreground">
                        <LocatorDot uuid={account.uuid} />
                        <span className="truncate">{account.name}</span>
                      </span>
                      <span className="block text-xs text-ink-muted">{t(`accounts.kind.${account.kind}`)}</span>
                    </span>
                    {active && <Check className="size-4" />}
                  </button>
                  <button
                    type="button"
                    aria-label={t("accounts.remove", { name: account.name })}
                    title={t("accounts.remove", { name: account.name })}
                    onClick={() => void removeAccount(account.id)}
                    className="grid size-9 place-items-center rounded-md text-ink-muted opacity-0 outline-none transition hover:bg-surface-2 hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
                  >
                    <Trash2 className="size-4" />
                  </button>
                </li>
              );
            })}
          </ul>
        )}

        <MicrosoftLogin />

        <div className="flex items-center gap-3 text-xs text-ink-muted" aria-hidden>
          <span className="h-px flex-1 bg-surface-3" />
          {t("accounts.or")}
          <span className="h-px flex-1 bg-surface-3" />
        </div>

        <form onSubmit={submit} className="flex flex-col gap-2">
          <label htmlFor="offline-nick" className="text-[13px] text-foreground">
            {t("accounts.offlineTitle")}
          </label>
          <div className="flex gap-2">
            <Input
              id="offline-nick"
              value={nick}
              onChange={(e) => {
                setNick(e.target.value);
                setError(null);
              }}
              placeholder={t("accounts.nickPlaceholder")}
              maxLength={16}
              autoComplete="off"
              spellCheck={false}
              aria-invalid={error !== null}
              className="h-10 bg-surface-0"
            />
            <Button type="submit" className="h-10">
              {t("accounts.addOffline")}
            </Button>
          </div>
          <p className={cn("text-xs", error ? "text-destructive" : "text-ink-muted")}>
            {error ?? t("accounts.offlineHint")}
          </p>
        </form>

      </DialogContent>
    </Dialog>
  );
}

function AccountHead({ account }: { account: Account }) {
  return <PlayerHead name={account.name} face={useAccountFace(account)} className="size-8" />;
}

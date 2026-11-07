import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, ExternalLink, KeyRound, Loader2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { contentApi } from "@/features/content/api";
import { backend, isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";

const CONSOLE = "https://console.curseforge.com/";

/**
 * Свой ключ CurseForge API. Без него работает вшитый в лаунчер (если есть), а без обоих —
 * только Modrinth. `onSaved` — после сохранения или удаления.
 */
export function CurseforgeKeyForm({ value, onSave }: { value: string | null; onSave: (key: string | null) => Promise<void> }) {
  const { t } = useTranslation();
  const [key, setKey] = useState(value ?? "");
  const [state, setState] = useState<"idle" | "checking" | "bad" | "saved">("idle");
  const [error, setError] = useState<string | null>(null);
  const [builtIn, setBuiltIn] = useState<boolean | null>(null);

  useEffect(() => setKey(value ?? ""), [value]);
  useEffect(() => {
    void contentApi.sources().then((s) => setBuiltIn(s.builtIn ?? false), () => setBuiltIn(false));
  }, []);

  const save = async () => {
    setError(null);
    const trimmed = key.trim();
    if (!trimmed) {
      await onSave(null);
      setState("idle");
      return;
    }
    setState("checking");
    try {
      if (!(await backend.checkCurseforgeKey(trimmed))) {
        setState("bad");
        return;
      }
      await onSave(trimmed);
      setState("saved");
    } catch (e) {
      setError(errorText(e));
      setState("idle");
    }
  };

  const status =
    value ? t("cfKey.statusOwn") : builtIn ? t("cfKey.statusBuiltIn") : builtIn === false ? t("cfKey.statusNone") : null;

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <label className="flex h-9 min-w-[240px] flex-1 items-center gap-2 rounded-md border border-input bg-surface-0 px-3 focus-within:border-ring">
          <KeyRound className="size-3.5 shrink-0 text-ink-muted" />
          <Input
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={key}
            onChange={(e) => {
              setKey(e.target.value);
              setState("idle");
            }}
            placeholder={t("cfKey.placeholder")}
            aria-label={t("cfKey.title")}
            className="h-full border-0 bg-transparent p-0 font-mono text-[12px] shadow-none focus-visible:ring-0"
          />
        </label>
        <Button size="sm" variant="secondary" disabled={state === "checking" || key.trim() === (value ?? "")} onClick={() => void save()}>
          {state === "checking" ? <Loader2 className="animate-spin" /> : <Check />}
          {key.trim() || !value ? t("cfKey.check") : t("cfKey.remove")}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => void (isTauri ? openUrl(CONSOLE) : window.open(CONSOLE))}>
          <ExternalLink /> {t("cfKey.get")}
        </Button>
      </div>
      {state === "bad" && (
        <p className="flex items-center gap-1.5 text-xs text-destructive">
          <X className="size-3.5" /> {t("cfKey.bad")}
        </p>
      )}
      {state === "saved" && (
        <p className="flex items-center gap-1.5 text-xs text-[#3fb950]">
          <Check className="size-3.5" /> {t("cfKey.saved")}
        </p>
      )}
      {error && <p className="text-xs text-destructive">{error}</p>}
      {status && state !== "saved" && <p className="text-xs text-ink-muted">{status}</p>}
    </div>
  );
}

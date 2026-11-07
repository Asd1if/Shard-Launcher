import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { showToast } from "@/components/toast";
import { isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { InstanceIcon } from "./instance-icon";
import { useInstances } from "./store";

type Part = "controls" | "sound" | "video";
const PARTS: Part[] = ["controls", "sound", "video"];

/** Перенос options.txt (управление, звук, графика) из этой сборки в другие. */
export function OptionsDialog({ id, name, onOpenChange }: { id: string; name: string; onOpenChange: (open: boolean) => void }) {
  const { t } = useTranslation();
  const others = useInstances().filter((i) => i.id !== id);
  const [parts, setParts] = useState<Part[]>(["controls", "sound"]);
  const [targets, setTargets] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const toggle = <T,>(list: T[], value: T) => (list.includes(value) ? list.filter((x) => x !== value) : [...list, value]);

  const apply = async () => {
    setBusy(true);
    try {
      const count = isTauri ? await invoke<number>("options_copy", { from: id, to: targets, parts }) : targets.length;
      showToast(t("options.done", { count }));
      onOpenChange(false);
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="border-surface-3 bg-surface-1 sm:max-w-[460px]">
        <DialogHeader>
          <DialogTitle>{t("options.title")}</DialogTitle>
          <DialogDescription>{t("options.text", { name })}</DialogDescription>
        </DialogHeader>
        <div className="flex flex-wrap gap-2">
          {PARTS.map((p) => (
            <label key={p} className="flex cursor-pointer items-center gap-2 rounded-md border border-surface-3 bg-surface-0 px-3 py-1.5 text-[13px]">
              <input type="checkbox" checked={parts.includes(p)} onChange={() => setParts((l) => toggle(l, p))} />
              {t(`options.part.${p}`)}
            </label>
          ))}
        </div>
        <div className="flex items-center justify-between text-xs text-ink-muted">
          <span>{t("options.to")}</span>
          {others.length > 1 && (
            <button type="button" className="hover:text-foreground" onClick={() => setTargets(targets.length === others.length ? [] : others.map((i) => i.id))}>
              {targets.length === others.length ? t("options.none") : t("options.all")}
            </button>
          )}
        </div>
        <div className="flex max-h-64 flex-col gap-0.5 overflow-y-auto">
          {others.length === 0 && <p className="text-[13px] text-ink-muted">{t("options.empty")}</p>}
          {others.map((i) => (
            <label key={i.id} className="flex cursor-pointer items-center gap-3 rounded-md px-2 py-1.5 hover:bg-surface-2">
              <input type="checkbox" checked={targets.includes(i.id)} onChange={() => setTargets((l) => toggle(l, i.id))} />
              <InstanceIcon name={i.name} loader={i.loader} icon={i.icon} className="size-7 [--step:1.5px]" />
              <span className="min-w-0 flex-1 truncate text-[13px]">{i.name}</span>
              <span className="text-xs text-ink-muted">{i.mcVersion}</span>
            </label>
          ))}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("create.cancel")}
          </Button>
          <Button disabled={busy || !parts.length || !targets.length} onClick={() => void apply()}>
            {busy && <Loader2 className="animate-spin" />} {t("options.apply")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, PackageCheck, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { showToast } from "@/components/toast";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { iconPresets } from "./icon-presets";
import { InstanceIcon, PresetIcon } from "./instance-icon";
import { refreshInstances, setInstanceIcon } from "./store";
import { backend } from "@/lib/backend";
import type { Instance } from "./types";

const SIZE = 128;

/** Картинку любого размера приводим к квадрату 128×128: обрезаем по центру, пиксель-арт не размываем. */
async function toIcon(file: File): Promise<string> {
  const url = URL.createObjectURL(file);
  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const i = new Image();
      i.onload = () => resolve(i);
      i.onerror = () => reject(new Error("icon_not_image"));
      i.src = url;
    });
    const side = Math.min(img.width, img.height);
    const canvas = document.createElement("canvas");
    canvas.width = SIZE;
    canvas.height = SIZE;
    const ctx = canvas.getContext("2d")!;
    // Маленькие картинки — скорее всего пиксель-арт: увеличиваем без сглаживания
    ctx.imageSmoothingEnabled = side >= SIZE;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(img, (img.width - side) / 2, (img.height - side) / 2, side, side, 0, 0, SIZE, SIZE);
    return canvas.toDataURL("image/png");
  } finally {
    URL.revokeObjectURL(url);
  }
}

export function IconPickerDialog({
  instance,
  open,
  onOpenChange,
}: {
  instance: Instance;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const file = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const current = instance.icon ?? null;

  const choose = async (icon: string | null) => {
    setBusy(true);
    try {
      await setInstanceIcon(instance.id, icon);
      onOpenChange(false);
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  // Оригинальная иконка модпака: скачиваем заново с Modrinth или CurseForge
  const original = async () => {
    setBusy(true);
    try {
      await backend.originalIcon(instance.id);
      await refreshInstances();
      onOpenChange(false);
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const tile = (selected: boolean) =>
    cn(
      "grid aspect-square place-items-center rounded-lg border bg-surface-0 p-2 outline-none transition-colors hover:border-ink-muted focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
      selected ? "border-brand" : "border-surface-3",
    );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[calc(100dvh-2rem)] flex-col gap-5 border-surface-3 bg-surface-1 sm:max-w-[460px]">
        <DialogHeader className="shrink-0">
          <DialogTitle className="font-display text-lg font-medium">{t("icon.title")}</DialogTitle>
          <DialogDescription>{t("icon.description")}</DialogDescription>
        </DialogHeader>

        <div role="listbox" aria-label={t("icon.title")} className="-mx-1 grid min-h-0 grid-cols-5 gap-2 overflow-y-auto px-1 py-1">
          <button
            type="button"
            role="option"
            aria-selected={current === null}
            title={t("icon.auto")}
            disabled={busy}
            onClick={() => void choose(null)}
            className={tile(current === null)}
          >
            <InstanceIcon name={instance.name} loader={instance.loader} className="size-full" />
          </button>
          {current?.startsWith("data:") && (
            <button type="button" role="option" aria-selected title={t("icon.custom")} className={tile(true)}>
              <InstanceIcon name={instance.name} loader={instance.loader} icon={current} className="size-full" />
            </button>
          )}
          {iconPresets.map((p) => {
            const value = `preset:${p.id}`;
            return (
              <button
                key={p.id}
                type="button"
                role="option"
                aria-selected={current === value}
                title={t(`icon.presets.${p.id}`)}
                disabled={busy}
                onClick={() => void choose(value)}
                className={tile(current === value)}
              >
                <PresetIcon preset={p} className="size-full" />
              </button>
            );
          })}
        </div>

        <DialogFooter className="shrink-0 items-center sm:justify-between">
          <div className="flex gap-2">
            <Button variant="secondary" disabled={busy} onClick={() => file.current?.click()}>
              {busy ? <Loader2 className="animate-spin" /> : <Upload />} {t("icon.upload")}
            </Button>
            {instance.pack?.iconUrl && (
              <Button variant="ghost" disabled={busy} onClick={() => void original()}>
                <PackageCheck /> {t("icon.original")}
              </Button>
            )}
          </div>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("icon.close")}
          </Button>
          <input
            ref={file}
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            className="hidden"
            onChange={(e) => {
              const f = e.target.files?.[0];
              e.target.value = "";
              if (f) void toIcon(f).then(choose, (err) => showToast(errorText(err)));
            }}
          />
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

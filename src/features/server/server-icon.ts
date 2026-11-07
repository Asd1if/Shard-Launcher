import { presetById } from "@/features/instances/icon-presets";
import type { Instance } from "@/features/instances/types";

/**
 * Иконка сборки → PNG 64×64 для server-icon.png (другого размера игра не принимает).
 * Своя картинка вписывается целиком, рисунок из набора рисуется по пикселям. Без иконки — null.
 */
export async function serverIconFor(instance: Instance): Promise<string | null> {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 64;
  const ctx = canvas.getContext("2d")!;
  ctx.imageSmoothingEnabled = false;
  const icon = instance.icon;

  if (icon?.startsWith("data:image/")) {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const el = new Image();
      el.onload = () => resolve(el);
      el.onerror = () => reject(new Error("icon_not_image"));
      el.src = icon;
    });
    // Маленькие пиксельные иконки увеличиваем без размытия, большие — сглаживаем
    ctx.imageSmoothingEnabled = img.width > 64;
    const scale = Math.min(64 / img.width, 64 / img.height);
    const w = img.width * scale;
    const h = img.height * scale;
    ctx.drawImage(img, (64 - w) / 2, (64 - h) / 2, w, h);
    return canvas.toDataURL("image/png");
  }

  const preset = icon?.startsWith("preset:") ? presetById(icon.slice(7)) : undefined;
  if (!preset) return null;
  ctx.fillStyle = preset.background;
  ctx.fillRect(0, 0, 64, 64);
  // 8×8 по 6 пикселей с полями 8 — как на плитке в лаунчере
  preset.rows.forEach((row, y) =>
    [...row].forEach((c, x) => {
      if (c === ".") return;
      ctx.fillStyle = preset.palette[c];
      ctx.fillRect(8 + x * 6, 8 + y * 6, 6, 6);
    }),
  );
  return canvas.toDataURL("image/png");
}

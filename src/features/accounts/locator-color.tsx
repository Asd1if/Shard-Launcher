import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

/**
 * Цвет игрока на панели локатора (Minecraft 1.21.6+), если сервер не задал свой.
 * Игра берёт `UUID.hashCode()` как RGB и выставляет яркость 0.9 в HSB — делаем так же.
 */
export function locatorColor(uuid: string): string | null {
  const hex = uuid.replace(/-/g, "");
  if (!/^[0-9a-f]{32}$/i.test(hex)) return null;
  const most = BigInt.asIntN(64, BigInt(`0x${hex.slice(0, 16)}`));
  const least = BigInt.asIntN(64, BigInt(`0x${hex.slice(16)}`));
  // Java: long hilo = most ^ least; return (int)(hilo >> 32) ^ (int) hilo;
  const hilo = most ^ least;
  const hash = Number(BigInt.asIntN(32, hilo >> 32n) ^ BigInt.asIntN(32, hilo));
  const r = (hash >> 16) & 0xff;
  const g = (hash >> 8) & 0xff;
  const b = hash & 0xff;

  // RGB → HSB, яркость 0.9, обратно в RGB (как java.awt.Color)
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const sat = max === 0 ? 0 : (max - min) / max;
  let hue = 0;
  if (sat !== 0) {
    const d = max - min;
    const rc = (max - r) / d;
    const gc = (max - g) / d;
    const bc = (max - b) / d;
    hue = r === max ? bc - gc : g === max ? 2 + rc - bc : 4 + gc - rc;
    hue /= 6;
    if (hue < 0) hue += 1;
  }
  const v = 0.9;
  const [ro, go, bo] = hsbToRgb(hue, sat, v);
  return `#${[ro, go, bo].map((c) => c.toString(16).padStart(2, "0")).join("")}`;
}

function hsbToRgb(h: number, s: number, v: number): [number, number, number] {
  if (s === 0) {
    const c = Math.floor(v * 255 + 0.5);
    return [c, c, c];
  }
  const hh = (h - Math.floor(h)) * 6;
  const f = hh - Math.floor(hh);
  const p = v * (1 - s);
  const q = v * (1 - s * f);
  const t = v * (1 - s * (1 - f));
  const table: [number, number, number][] = [
    [v, t, p],
    [q, v, p],
    [p, v, t],
    [p, q, v],
    [t, p, v],
    [v, p, q],
  ];
  return table[Math.floor(hh)].map((c) => Math.floor(c * 255 + 0.5)) as [number, number, number];
}

/** Точка перед ником — цвет, которым игрока видят другие на панели локатора. */
export function LocatorDot({ uuid, className }: { uuid: string; className?: string }) {
  const { t } = useTranslation();
  const color = locatorColor(uuid);
  if (!color) return null;
  return (
    <span
      role="img"
      aria-label={t("accounts.locator", { color })}
      title={t("accounts.locator", { color })}
      className={cn("inline-block size-2 shrink-0 rounded-full ring-1 ring-black/20", className)}
      style={{ backgroundColor: color }}
    />
  );
}

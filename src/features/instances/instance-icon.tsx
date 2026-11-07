import { cn } from "@/lib/utils";
import { presetById, type IconPreset } from "./icon-presets";

// Цвета по загрузчику: у каждой сборки свой узор, но оттенок подсказывает загрузчик.
const loaderPalettes: Record<string, [string, string, string]> = {
  vanilla: ["#5d8a3a", "#7cb342", "#3e5f26"],
  fabric: ["#c9b38f", "#e8d7b5", "#8a7656"],
  quilt: ["#9b6bd8", "#c49bff", "#5f3a96"],
  forge: ["#d0714a", "#f0a07a", "#8a4127"],
  neoforge: ["#e2a33a", "#ffd27a", "#9a6618"],
};

function hash(input: string) {
  let h = 2166136261;
  for (let i = 0; i < input.length; i++) {
    h ^= input.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/** Встроенный пиксельный рисунок 8×8 на цветной плитке. */
export function PresetIcon({ preset, className }: { preset: IconPreset; className?: string }) {
  return (
    <div
      aria-hidden
      className={cn("pixel-corners relative shrink-0 [--step:2px]", className)}
      style={{ backgroundColor: preset.background }}
    >
      {/* Отступ рисунка от края плитки — в процентах от самой плитки, а не от родителя */}
      <div className="absolute inset-[12%] grid grid-cols-8 grid-rows-8">
        {preset.rows.flatMap((row, y) =>
          [...row].map((c, x) => <span key={`${y}-${x}`} style={c === "." ? undefined : { backgroundColor: preset.palette[c] }} />),
        )}
      </div>
    </div>
  );
}

/**
 * Иконка сборки: своя картинка, рисунок из набора или — по умолчанию —
 * пиксельный узор 4×4 из названия, оттенок которого подсказывает загрузчик.
 */
export function InstanceIcon({
  name,
  loader,
  icon,
  className,
}: {
  name: string;
  loader: string | null;
  icon?: string | null;
  className?: string;
}) {
  if (icon?.startsWith("data:image/")) {
    return (
      <img
        src={icon}
        alt=""
        aria-hidden
        draggable={false}
        className={cn("pixel-corners shrink-0 bg-surface-2 object-cover [--step:2px] [image-rendering:pixelated]", className)}
      />
    );
  }
  const preset = icon?.startsWith("preset:") ? presetById(icon.slice(7)) : undefined;
  if (preset) return <PresetIcon preset={preset} className={className} />;

  const palette = loaderPalettes[loader ?? "vanilla"] ?? loaderPalettes.vanilla;
  let h = hash(name);
  const cells: string[] = [];
  for (let row = 0; row < 4; row++) {
    const left: string[] = [];
    for (let col = 0; col < 2; col++) {
      left.push(palette[h % 3]);
      h = Math.floor(h / 3) || hash(name + row + col);
    }
    cells.push(...left, ...[...left].reverse());
  }

  return (
    <div
      aria-hidden
      className={cn("pixel-corners grid shrink-0 grid-cols-4 [--step:2px]", className)}
    >
      {cells.map((color, i) => (
        <span key={i} style={{ backgroundColor: color }} />
      ))}
    </div>
  );
}

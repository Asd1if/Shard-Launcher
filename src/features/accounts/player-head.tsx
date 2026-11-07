import { cn } from "@/lib/utils";

// Оттенки «кожи» и «волос» для пиксельной головы оффлайн-игрока (скинов в оффлайне нет).
const skins = ["#c98e6b", "#b07552", "#e0b090", "#8d5a3b", "#f0c8a0"];
const hairs = ["#3b2a1e", "#6b4a2b", "#1f1b18", "#a0702e", "#d9b26a", "#5a3a5e"];

function hash(input: string) {
  let h = 0;
  for (let i = 0; i < input.length; i++) h = (Math.imul(h, 31) + input.charCodeAt(i)) >>> 0;
  return h;
}

/** Голова 8×8 в стиле Minecraft: лицо со скина лицензии (`face`) или нарисованная по нику. */
export function PlayerHead({ name, face, className }: { name: string; face?: string | null; className?: string }) {
  if (face) {
    return <img src={face} alt="" aria-hidden className={cn("pixel-corners shrink-0 [image-rendering:pixelated] [--step:1px]", className)} />;
  }
  const h = hash(name);
  const skin = skins[h % skins.length];
  const hair = hairs[(h >>> 4) % hairs.length];
  const eye = "#2c3e8f";
  // 8×8: 0 — кожа, 1 — волосы, 2 — глаз, 3 — белок
  const rows = [
    "11111111",
    "11111111",
    "10000001",
    "00000000",
    "03200230",
    "00000000",
    "00011000",
    "00000000",
  ];
  const color = ["", hair, eye, "#ffffff"];
  return (
    <svg viewBox="0 0 8 8" shapeRendering="crispEdges" aria-hidden className={cn("pixel-corners shrink-0 [--step:1px]", className)}>
      <rect width="8" height="8" fill={skin} />
      {rows.flatMap((row, y) =>
        [...row].map((c, x) => (c === "0" ? null : <rect key={`${x}${y}`} x={x} y={y} width="1" height="1" fill={color[+c]} />)),
      )}
    </svg>
  );
}

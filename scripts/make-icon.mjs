// Генерирует src-tauri/icons/app-icon.svg из src/brand/shard-pixels.json.
// Затем: npx tauri icon src-tauri/icons/app-icon.svg
import { readFileSync, writeFileSync } from "node:fs";

const art = JSON.parse(readFileSync(new URL("../src/brand/shard-pixels.json", import.meta.url), "utf8"));
const unit = 64; // 16 × 64 = 1024
const s = art.size * unit;

// Плитка со ступенчатыми углами, шаг = полклетки
const step = unit / 2;
const tile = [
  [0, 2 * step], [step, 2 * step], [step, step], [2 * step, step], [2 * step, 0],
  [s - 2 * step, 0], [s - 2 * step, step], [s - step, step], [s - step, 2 * step], [s, 2 * step],
  [s, s - 2 * step], [s - step, s - 2 * step], [s - step, s - step], [s - 2 * step, s - step], [s - 2 * step, s],
  [2 * step, s], [2 * step, s - step], [step, s - step], [step, s - 2 * step], [0, s - 2 * step],
]
  .map(([x, y]) => `${x},${y}`)
  .join(" ");

const rects = [];
art.rows.forEach((row, y) => {
  [...row].forEach((ch, x) => {
    const color = art.colors[ch];
    if (color) rects.push(`<rect x="${x * unit}" y="${y * unit}" width="${unit}" height="${unit}" fill="${color}"/>`);
  });
});

const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${s} ${s}" width="${s}" height="${s}" shape-rendering="crispEdges">
<polygon points="${tile}" fill="${art.tile}"/>
${rects.join("\n")}
</svg>
`;

writeFileSync(new URL("../src-tauri/icons/app-icon.svg", import.meta.url), svg);
console.log("icons/app-icon.svg written");

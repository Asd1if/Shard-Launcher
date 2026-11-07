import { useEffect, useRef, useSyncExternalStore } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { isTauri } from "@/lib/backend";
import { cn } from "@/lib/utils";
import { refreshTheme } from "@/theme";

/** Фон лаунчера: за всеми страницами и боковым меню. */
export const backdrops = ["aurora", "mesh", "pixels", "stars", "snow", "rain", "landscape", "grid", "custom", "none"] as const;
export type BackdropKind = (typeof backdrops)[number];

export interface BackdropPrefs {
  kind: BackdropKind;
  /** Затемнение поверх фона, 0–80 % — чтобы текст читался на любой картинке */
  dim: number;
  /** Полный путь к своей картинке */
  custom: string | null;
  /** Средняя яркость своей картинки 0–1 — по ней тема «Авто» выбирает светлую или тёмную */
  luma: number | null;
}

const KEY = "shard.backdrop";
const listeners = new Set<() => void>();
let prefs: BackdropPrefs = read();

function read(): BackdropPrefs {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (v && backdrops.includes(v.kind))
      return {
        kind: v.kind,
        dim: Number(v.dim) || 0,
        custom: v.custom ?? null,
        luma: typeof v.luma === "number" ? v.luma : null,
      };
  } catch {
    // нет хранилища или мусор — берём по умолчанию
  }
  return { kind: "aurora", dim: 0, custom: null, luma: null };
}

export function setBackdrop(next: Partial<BackdropPrefs>) {
  prefs = { ...prefs, ...next };
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    // просто не запомнится
  }
  listeners.forEach((fn) => fn());
  refreshTheme();
}

export const useBackdrop = () =>
  useSyncExternalStore(
    (fn) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    () => prefs,
  );

/** Выбрать свою картинку: копия ляжет в папку лаунчера. */
export async function pickBackdropImage(title: string) {
  if (!isTauri) return;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const path = await open({ title, multiple: false, filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }] });
  if (typeof path !== "string") return;
  const stored = await invoke<string | null>("background_set", { path });
  setBackdrop({ kind: "custom", custom: stored, luma: stored ? await measureLuma(stored) : null });
}

/** Средняя яркость картинки: считает бэкенд, чтобы не упереться в CORS у холста. */
async function measureLuma(path: string): Promise<number | null> {
  if (isTauri) return invoke<number>("background_luma", { path }).catch(() => null);
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      try {
        const c = document.createElement("canvas");
        c.width = c.height = 32;
        const ctx = c.getContext("2d")!;
        ctx.drawImage(img, 0, 0, 32, 32);
        const d = ctx.getImageData(0, 0, 32, 32).data;
        let sum = 0;
        for (let i = 0; i < d.length; i += 4) sum += (0.2126 * d[i] + 0.7152 * d[i + 1] + 0.0722 * d[i + 2]) / 255;
        resolve(sum / (d.length / 4));
      } catch {
        resolve(null);
      }
    };
    img.onerror = () => resolve(null);
    img.src = path;
  });
}

/** Считает яркость своей картинки, если её ещё нет, — для темы «Авто». */
export function useBackdropTone() {
  const { kind, custom, luma } = useBackdrop();
  useEffect(() => {
    if (kind === "custom" && custom && luma === null) {
      void measureLuma(custom).then((v) => v !== null && prefs.custom === custom && setBackdrop({ luma: v }));
    }
  }, [kind, custom, luma]);
}

const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Цвета активной темы — фон подстраивается под графит, аметист и светлую. */
function themeColors() {
  const css = getComputedStyle(document.documentElement);
  const get = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
  return { brand: get("--brand", "#e4e4e7"), surface: get("--surface-2", "#222226"), bg: get("--background", "#141416"), ink: get("--ink", "#ececee") };
}

export function Backdrop({ kind, className, dim, custom }: BackdropPrefs & { className?: string }) {
  if (kind === "none") return null;
  return (
    <div aria-hidden className={cn("pointer-events-none absolute inset-0 overflow-hidden", className)}>
      {kind === "aurora" && <Aurora />}
      {kind === "pixels" && <PixelField />}
      {kind === "landscape" && <Landscape />}
      {kind === "mesh" && <Mesh />}
      {kind === "stars" && <Stars />}
      {kind === "snow" && <Snow />}
      {kind === "rain" && <Rain />}
      {kind === "grid" && <Grid />}
      {kind === "custom" && custom && (
        <img src={isTauri ? convertFileSrc(custom) : custom} alt="" className="absolute inset-0 size-full object-cover" />
      )}
      {/* Лёгкая вуаль, чтобы текст страниц читался на любом фоне */}
      <div className="absolute inset-0 bg-background/35" />
      {dim > 0 && <div className="absolute inset-0 bg-background" style={{ opacity: dim / 100 }} />}
    </div>
  );
}

/** Медленно плывущие размытые пятна цвета темы. */
function Aurora() {
  return (
    <div className="absolute inset-0 opacity-70">
      <div className="backdrop-blob absolute -top-1/4 -left-1/4 size-[70%] rounded-full bg-brand/40 blur-3xl [animation-duration:26s]" />
      <div className="backdrop-blob absolute top-0 right-[-20%] size-[60%] rounded-full bg-[#3b82f6]/25 blur-3xl [animation-delay:-8s] [animation-duration:32s]" />
      <div className="backdrop-blob absolute bottom-[-30%] left-1/4 size-[65%] rounded-full bg-[#10b981]/20 blur-3xl [animation-delay:-15s] [animation-duration:38s]" />
    </div>
  );
}

/** Пиксели-«искры», которые медленно поднимаются и мерцают. */
function PixelField() {
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const el = canvas.current!;
    const ctx = el.getContext("2d")!;
    const { brand } = themeColors();
    const still = reducedMotion();
    let frame = 0;
    let w = 0;
    let h = 0;
    type Spark = { x: number; y: number; s: number; v: number; p: number };
    let sparks: Spark[] = [];

    const resize = () => {
      const r = el.getBoundingClientRect();
      w = el.width = Math.max(1, Math.floor(r.width / 2));
      h = el.height = Math.max(1, Math.floor(r.height / 2));
      sparks = Array.from({ length: Math.round((w * h) / 900) }, () => ({
        x: Math.random() * w,
        y: Math.random() * h,
        s: Math.random() < 0.15 ? 3 : Math.random() < 0.5 ? 2 : 1,
        v: 0.05 + Math.random() * 0.2,
        p: Math.random() * Math.PI * 2,
      }));
    };
    const draw = (time: number) => {
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = brand;
      for (const s of sparks) {
        if (!still) {
          s.y -= s.v;
          if (s.y < -4) {
            s.y = h + 4;
            s.x = Math.random() * w;
          }
        }
        ctx.globalAlpha = 0.25 + 0.35 * (0.5 + 0.5 * Math.sin(time / 900 + s.p)) * (s.y / h);
        ctx.fillRect(Math.round(s.x), Math.round(s.y), s.s, s.s);
      }
      if (!still) frame = requestAnimationFrame(draw);
    };
    const onResize = () => {
      resize();
      if (still) draw(0);
    };
    resize();
    const observer = new ResizeObserver(onResize);
    observer.observe(el);
    frame = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, []);

  return <canvas ref={canvas} className="absolute inset-0 size-full [image-rendering:pixelated]" />;
}

/** Пиксельные холмы в три слоя и облака, которые плывут с разной скоростью. */
function Landscape() {
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const el = canvas.current!;
    const ctx = el.getContext("2d")!;
    const { brand, surface } = themeColors();
    const still = reducedMotion();
    let frame = 0;
    let w = 0;
    let h = 0;
    const PX = 4;
    // Высота холма из суммы синусов — без шума Перлина, но выглядит естественно
    const hill = (x: number, seed: number, amp: number) =>
      Math.sin(x * 0.013 + seed) * amp + Math.sin(x * 0.031 + seed * 2.1) * amp * 0.45 + Math.sin(x * 0.07 + seed * 3.7) * amp * 0.15;
    const clouds = Array.from({ length: 7 }, (_, i) => ({ x: Math.random() * 400, y: 8 + i * 6 + Math.random() * 10, len: 6 + Math.round(Math.random() * 10) }));

    const resize = () => {
      const r = el.getBoundingClientRect();
      w = el.width = Math.max(1, Math.floor(r.width / PX));
      h = el.height = Math.max(1, Math.floor(r.height / PX));
    };
    const layers = [
      { base: 0.55, amp: 7, seed: 1.3, speed: 0.02, alpha: 0.18 },
      { base: 0.68, amp: 9, seed: 4.1, speed: 0.05, alpha: 0.28 },
      { base: 0.82, amp: 6, seed: 7.7, speed: 0.12, alpha: 0.5 },
    ];
    const draw = (time: number) => {
      const t = still ? 0 : time / 16;
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = "#ffffff";
      for (const c of clouds) {
        const x = Math.round(((c.x + t * 0.03) % (w + c.len * 2)) - c.len);
        ctx.globalAlpha = 0.08;
        ctx.fillRect(x, Math.round(c.y * (h / 80)), c.len, 2);
        ctx.fillRect(x + 2, Math.round(c.y * (h / 80)) - 1, c.len - 4, 1);
      }
      for (const layer of layers) {
        ctx.globalAlpha = layer.alpha;
        const shift = t * layer.speed;
        for (let x = 0; x < w; x++) {
          const top = Math.round(h * layer.base + hill(x + shift, layer.seed, layer.amp));
          // Верхний пиксель — «трава» цвета темы, ниже — «земля»
          ctx.fillStyle = brand;
          ctx.fillRect(x, top, 1, 1);
          ctx.fillStyle = surface;
          ctx.fillRect(x, top + 1, 1, h - top);
        }
      }
      if (!still) frame = requestAnimationFrame(draw);
    };
    const onResize = () => {
      resize();
      if (still) draw(0);
    };
    resize();
    const observer = new ResizeObserver(onResize);
    observer.observe(el);
    frame = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, []);

  return <canvas ref={canvas} className="absolute inset-0 size-full [image-rendering:pixelated]" />;
}


/** Цвет «#rrggbb» с прозрачностью — для градиентов на холсте. */
function alpha(color: string, a: number) {
  const m = /^#([0-9a-f]{6})$/i.exec(color.trim());
  if (!m) return color;
  const n = parseInt(m[1], 16);
  return `rgba(${n >> 16}, ${(n >> 8) & 255}, ${n & 255}, ${a})`;
}

type Scene = { resize: (w: number, h: number) => void; draw: (time: number) => void };

/**
 * Общий цикл для фонов на холсте: размер под окно (с понижением разрешения для пикселей),
 * перерисовка по кадрам, один статичный кадр при «уменьшить движение».
 */
function CanvasScene({ scale, setup }: { scale: number; setup: (ctx: CanvasRenderingContext2D, colors: ReturnType<typeof themeColors>) => Scene }) {
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const el = canvas.current!;
    const ctx = el.getContext("2d")!;
    const scene = setup(ctx, themeColors());
    const still = reducedMotion();
    let frame = 0;
    const resize = () => {
      const r = el.getBoundingClientRect();
      el.width = Math.max(1, Math.floor(r.width / scale));
      el.height = Math.max(1, Math.floor(r.height / scale));
      scene.resize(el.width, el.height);
      if (still) scene.draw(0);
    };
    const loop = (time: number) => {
      scene.draw(time);
      frame = requestAnimationFrame(loop);
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(el);
    if (!still) frame = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
    // setup — статичная функция сцены
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <canvas ref={canvas} className="absolute inset-0 size-full [image-rendering:pixelated]" />;
}

/** Мягкий градиент: три больших пятна цвета темы медленно ходят по кругу. */
function Mesh() {
  return (
    <CanvasScene
      scale={8}
      setup={(ctx, { brand, surface }) => {
        let w = 1;
        let h = 1;
        const spots = [
          { color: brand, a: 0.35, r: 0.7, speed: 0.00005, phase: 0 },
          { color: "#6366f1", a: 0.22, r: 0.6, speed: 0.00007, phase: 2.1 },
          { color: "#ec4899", a: 0.16, r: 0.55, speed: 0.00004, phase: 4.2 },
        ];
        return {
          resize: (nw, nh) => ((w = nw), (h = nh)),
          draw: (t) => {
            ctx.clearRect(0, 0, w, h);
            ctx.fillStyle = alpha(surface, 0.4);
            ctx.fillRect(0, 0, w, h);
            for (const s of spots) {
              const a = t * s.speed + s.phase;
              const x = w * (0.5 + 0.32 * Math.cos(a));
              const y = h * (0.5 + 0.3 * Math.sin(a * 1.3));
              const r = Math.max(w, h) * s.r;
              const g = ctx.createRadialGradient(x, y, 0, x, y, r);
              g.addColorStop(0, alpha(s.color, s.a));
              g.addColorStop(1, alpha(s.color, 0));
              ctx.fillStyle = g;
              ctx.fillRect(0, 0, w, h);
            }
          },
        };
      }}
    />
  );
}

/** Ночное небо: мерцающие звёзды и изредка падающая звезда. */
function Stars() {
  return (
    <CanvasScene
      scale={2}
      setup={(ctx, { brand, ink }) => {
        let w = 1;
        let h = 1;
        let stars: { x: number; y: number; s: number; p: number; c: string }[] = [];
        let meteor: { x: number; y: number; life: number } | null = null;
        let next = 3000;
        return {
          resize: (nw, nh) => {
            w = nw;
            h = nh;
            stars = Array.from({ length: Math.round((w * h) / 1400) }, () => ({
              x: Math.random() * w,
              y: Math.random() * h,
              s: Math.random() < 0.08 ? 2 : 1,
              p: Math.random() * Math.PI * 2,
              c: Math.random() < 0.25 ? brand : ink,
            }));
          },
          draw: (t) => {
            ctx.clearRect(0, 0, w, h);
            for (const s of stars) {
              ctx.globalAlpha = 0.25 + 0.55 * (0.5 + 0.5 * Math.sin(t / 700 + s.p));
              ctx.fillStyle = s.c;
              ctx.fillRect(Math.round(s.x), Math.round(s.y), s.s, s.s);
            }
            if (!meteor && t > next) meteor = { x: Math.random() * w * 0.7 + w * 0.2, y: Math.random() * h * 0.3, life: 1 };
            if (meteor) {
              meteor.x -= 3;
              meteor.y += 1.5;
              meteor.life -= 0.012;
              for (let i = 0; i < 18; i++) {
                ctx.globalAlpha = meteor.life * (1 - i / 18) * 0.8;
                ctx.fillStyle = ink;
                ctx.fillRect(Math.round(meteor.x + i * 2), Math.round(meteor.y - i), 1, 1);
              }
              if (meteor.life <= 0) {
                meteor = null;
                next = t + 4000 + Math.random() * 6000;
              }
            }
            ctx.globalAlpha = 1;
          },
        };
      }}
    />
  );
}

/** Пиксельный снег: хлопья падают с покачиванием, ближние крупнее и быстрее. */
function Snow() {
  return (
    <CanvasScene
      scale={2}
      setup={(ctx, { ink }) => {
        let w = 1;
        let h = 1;
        let flakes: { x: number; y: number; z: number; p: number }[] = [];
        return {
          resize: (nw, nh) => {
            w = nw;
            h = nh;
            flakes = Array.from({ length: Math.round((w * h) / 1800) }, () => ({ x: Math.random() * w, y: Math.random() * h, z: Math.random(), p: Math.random() * 6 }));
          },
          draw: (t) => {
            ctx.clearRect(0, 0, w, h);
            ctx.fillStyle = ink;
            for (const f of flakes) {
              f.y += 0.15 + f.z * 0.45;
              const x = f.x + Math.sin(t / 1800 + f.p) * (4 + f.z * 6);
              if (f.y > h + 2) {
                f.y = -2;
                f.x = Math.random() * w;
              }
              ctx.globalAlpha = 0.25 + f.z * 0.5;
              const size = f.z > 0.75 ? 2 : 1;
              ctx.fillRect(Math.round(x), Math.round(f.y), size, size);
            }
            ctx.globalAlpha = 1;
          },
        };
      }}
    />
  );
}

/** Косой пиксельный дождь цвета темы. */
function Rain() {
  return (
    <CanvasScene
      scale={2}
      setup={(ctx, { brand }) => {
        let w = 1;
        let h = 1;
        let drops: { x: number; y: number; v: number; len: number }[] = [];
        return {
          resize: (nw, nh) => {
            w = nw;
            h = nh;
            drops = Array.from({ length: Math.round((w * h) / 2600) }, () => ({ x: Math.random() * w, y: Math.random() * h, v: 2 + Math.random() * 3, len: 4 + Math.round(Math.random() * 6) }));
          },
          draw: () => {
            ctx.clearRect(0, 0, w, h);
            ctx.fillStyle = brand;
            for (const d of drops) {
              d.y += d.v;
              d.x -= d.v * 0.25;
              if (d.y > h + d.len) {
                d.y = -d.len;
                d.x = Math.random() * (w + h * 0.25);
              }
              ctx.globalAlpha = 0.12 + d.v * 0.05;
              for (let i = 0; i < d.len; i++) ctx.fillRect(Math.round(d.x + i * 0.25), Math.round(d.y - i), 1, 1);
            }
            ctx.globalAlpha = 1;
          },
        };
      }}
    />
  );
}

/** Ретро-сетка: пол из линий уходит к горизонту, над ним свечение цвета темы. */
function Grid() {
  return (
    <CanvasScene
      scale={2}
      setup={(ctx, { brand }) => {
        let w = 1;
        let h = 1;
        return {
          resize: (nw, nh) => ((w = nw), (h = nh)),
          draw: (t) => {
            ctx.clearRect(0, 0, w, h);
            const horizon = Math.round(h * 0.58);
            const glow = ctx.createLinearGradient(0, horizon - h * 0.35, 0, horizon);
            glow.addColorStop(0, alpha(brand, 0));
            glow.addColorStop(1, alpha(brand, 0.22));
            ctx.fillStyle = glow;
            ctx.fillRect(0, 0, w, horizon);
            ctx.fillStyle = brand;
            // Горизонтальные линии бегут к зрителю
            const shift = (t / 2400) % 1;
            for (let i = 0; i < 14; i++) {
              const k = (i + shift) / 14;
              const y = Math.round(horizon + (h - horizon) * k * k);
              ctx.globalAlpha = 0.1 + 0.35 * k;
              ctx.fillRect(0, y, w, 1);
            }
            // Лучи из точки схода
            const cx = w / 2;
            for (let i = -12; i <= 12; i++) {
              const bottom = cx + i * (w / 10);
              for (let y = horizon; y < h; y += 1) {
                const k = (y - horizon) / (h - horizon);
                ctx.globalAlpha = 0.08 + 0.3 * k;
                ctx.fillRect(Math.round(cx + (bottom - cx) * k), y, 1, 1);
              }
            }
            ctx.globalAlpha = 1;
          },
        };
      }}
    />
  );
}

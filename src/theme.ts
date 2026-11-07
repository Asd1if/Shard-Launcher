import { useSyncExternalStore } from "react";

/** Основные: тёмная (graphite — старое имя, чтобы сохранённый выбор не сбился) и светлая. */
export const mainThemes = ["graphite", "light"] as const;
/** Дополнительные — тёмные с цветным акцентом. */
export const extraThemes = ["amethyst", "emerald", "amber", "ocean", "rose"] as const;
export const themes = [...mainThemes, ...extraThemes] as const;
export type Theme = (typeof themes)[number];

/**
 * Что выбрал человек: конкретная тема или правило.
 * system — как в Windows; auto — по яркости своей картинки фона (без картинки — как system).
 */
export const themeModes = ["system", "auto"] as const;
export type ThemeChoice = Theme | (typeof themeModes)[number];
const choices: readonly string[] = [...themes, ...themeModes];

// Тот же ключ читает inline-скрипт в index.html, чтобы тема применилась до первого кадра.
const STORAGE_KEY = "shard.theme";
const BACKDROP_KEY = "shard.backdrop";
const listeners = new Set<() => void>();
const media = window.matchMedia("(prefers-color-scheme: dark)");

function readChoice(): ThemeChoice {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v && choices.includes(v)) return v as ThemeChoice;
  } catch {
    // нет хранилища — по умолчанию
  }
  return "graphite";
}

let choice: ThemeChoice = readChoice();

const systemTheme = (): Theme => (media.matches ? "graphite" : "light");

/** Тема по картинке фона: светлая картинка — светлая тема. */
function autoTheme(): Theme {
  try {
    const b = JSON.parse(localStorage.getItem(BACKDROP_KEY) ?? "null");
    if (b?.kind === "custom" && b.custom && typeof b.luma === "number") return b.luma > 0.52 ? "light" : "graphite";
  } catch {
    // нет данных о фоне
  }
  return systemTheme();
}

function resolve(c: ThemeChoice): Theme {
  return c === "system" ? systemTheme() : c === "auto" ? autoTheme() : c;
}

function apply() {
  const next = resolve(choice);
  if (document.documentElement.dataset.theme !== next) document.documentElement.dataset.theme = next;
  listeners.forEach((fn) => fn());
}

export function setTheme(next: ThemeChoice) {
  choice = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // не критично: тема просто не запомнится
  }
  apply();
}

/** Пересчитать тему: сменилась картинка фона или её яркость (для режима auto). */
export function refreshTheme() {
  if (choice === "auto") apply();
}

media.addEventListener("change", () => {
  if (choice === "system" || choice === "auto") apply();
});
apply();

const subscribe = (fn: () => void) => {
  listeners.add(fn);
  return () => void listeners.delete(fn);
};

/** Применённая тема (для цветов и перерисовки фона). */
export function useTheme() {
  return useSyncExternalStore(subscribe, () => resolve(choice));
}

/** Выбор человека — для переключателей в настройках. */
export function useThemeChoice() {
  return useSyncExternalStore(subscribe, () => choice);
}

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Лаунчер запущен на Windows (для подсказок с путями и т. п.). */
export const isWindows = typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent);

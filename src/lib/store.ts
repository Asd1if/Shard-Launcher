import { useSyncExternalStore } from "react";

/** Маленькое глобальное хранилище для useSyncExternalStore. */
export function createStore<T>(initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  const subscribe = (fn: () => void) => {
    listeners.add(fn);
    return () => void listeners.delete(fn);
  };
  return {
    get: () => value,
    set(next: T | ((prev: T) => T)) {
      value = typeof next === "function" ? (next as (prev: T) => T)(value) : next;
      listeners.forEach((fn) => fn());
    },
    use: () => useSyncExternalStore(subscribe, () => value),
  };
}

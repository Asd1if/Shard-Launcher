import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { createStore } from "@/lib/store";

const store = createStore<{ id: number; text: string } | null>(null);
let timer: ReturnType<typeof setTimeout> | undefined;

/** Одно сообщение внизу справа; новое заменяет старое. */
export function showToast(text: string) {
  clearTimeout(timer);
  store.set({ id: Date.now(), text });
  timer = setTimeout(() => store.set(null), 8000);
}

export function Toaster() {
  const { t } = useTranslation();
  const toast = store.use();
  if (!toast) return null;
  return (
    <div
      key={toast.id}
      role="alert"
      className="fixed right-5 bottom-5 z-50 flex max-w-[420px] animate-in items-start gap-3 rounded-lg border border-surface-3 bg-surface-2 py-3 pr-2 pl-4 text-[13px] shadow-lg fade-in slide-in-from-bottom-2"
    >
      <p className="flex-1 break-words">{toast.text}</p>
      <button
        type="button"
        aria-label={t("window.close")}
        onClick={() => store.set(null)}
        className="grid size-6 shrink-0 place-items-center rounded text-ink-muted hover:bg-surface-3 hover:text-foreground"
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}

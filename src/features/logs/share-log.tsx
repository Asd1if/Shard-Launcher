import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Loader2, Share2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { showToast } from "@/components/toast";
import { isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";

/** Загружает лог на mclo.gs. content — текст; без него берётся logs/latest.log сборки. */
export async function uploadLog(args: { content?: string; instanceId?: string }) {
  if (!isTauri) return `https://mclo.gs/preview${Math.random().toString(36).slice(2, 8)}`;
  const { url } = await invoke<{ url: string }>("logs_upload", { content: args.content ?? null, instanceId: args.instanceId ?? null });
  return url;
}

/** Кнопка «mclo.gs»: загрузить, скопировать ссылку и открыть её. */
export function ShareLogButton({ content, instanceId, size = "sm" }: { content?: () => string; instanceId?: string; size?: "sm" | "icon" }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);

  const share = async () => {
    setBusy(true);
    try {
      const url = await uploadLog({ content: content?.(), instanceId });
      await navigator.clipboard.writeText(url).catch(() => {});
      showToast(t("mclogs.done", { url }));
      void (isTauri ? openUrl(url) : window.open(url));
    } catch (e) {
      showToast(t("mclogs.failed", { error: errorText(e) }));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Button variant="ghost" size={size === "icon" ? "icon" : "sm"} disabled={busy} title={t("mclogs.hint")} aria-label={t("mclogs.share")} onClick={() => void share()}>
      {busy ? <Loader2 className="animate-spin" /> : <Share2 />}
      {size !== "icon" && t("mclogs.share")}
    </Button>
  );
}

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Copy, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { showToast } from "@/components/toast";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";

export const MCP_PORT = 8542;
export const mcpUrl = (token: string) => `http://127.0.0.1:${MCP_PORT}/mcp?token=${token}`;

type Format = "json" | "claude" | "stdio";

/** Готовые настройки для разных MCP-клиентов. Сервер — обычный MCP по Streamable HTTP,
 *  так что подойдёт любой клиент; stdio-клиентам — через мост mcp-remote. */
function snippet(format: Format, url: string) {
  switch (format) {
    case "json":
      return JSON.stringify({ mcpServers: { shard: { type: "http", url } } }, null, 2);
    case "claude":
      return `claude mcp add --transport http shard "${url}"`;
    case "stdio":
      return JSON.stringify({ mcpServers: { shard: { command: "npx", args: ["-y", "mcp-remote", url] } } }, null, 2);
  }
}

const FORMATS: Format[] = ["json", "claude", "stdio"];

/** Ссылка на локальный MCP-сервер и примеры подключения. Токен на экране скрыт, копируется целиком. */
export function McpConnect({ token, onRegenerate }: { token: string; onRegenerate: () => void }) {
  const { t } = useTranslation();
  const [format, setFormat] = useState<Format>("json");
  const url = mcpUrl(token);
  const masked = mcpUrl("•".repeat(12));

  const copy = async (text: string, message: string) => {
    try {
      await navigator.clipboard.writeText(text);
      showToast(message);
    } catch (e) {
      showToast(errorText(e));
    }
  };

  return (
    <div className="flex flex-col gap-3 px-5">
      <div className="flex flex-wrap items-center gap-2">
        <Input type="password" readOnly value={url} aria-label={t("launcher.mcpUrl")} className="h-9 min-w-0 flex-1 bg-surface-0 font-mono text-[12px]" />
        <Button variant="ghost" size="sm" onClick={() => void copy(url, t("launcher.mcpUrlCopied"))}>
          <Copy /> {t("launcher.mcpCopyUrl")}
        </Button>
        <Button variant="ghost" size="sm" onClick={onRegenerate}>
          <RotateCcw /> {t("launcher.mcpRegenerate")}
        </Button>
      </div>

      <div className="flex flex-col gap-2 rounded-md border border-surface-3 bg-surface-0 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div role="tablist" aria-label={t("launcher.mcpSetup")} className="flex rounded-md bg-surface-2 p-0.5">
            {FORMATS.map((f) => (
              <button
                key={f}
                type="button"
                role="tab"
                aria-selected={format === f}
                onClick={() => setFormat(f)}
                className={cn(
                  "h-6 rounded px-2.5 text-xs text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                  format === f && "bg-foreground font-medium text-background hover:text-background",
                )}
              >
                {t(`launcher.mcpFormats.${f}`)}
              </button>
            ))}
          </div>
          <Button variant="ghost" size="sm" onClick={() => void copy(snippet(format, url), t("launcher.mcpConfigCopied"))}>
            <Copy /> {t("launcher.mcpCopyConfig")}
          </Button>
        </div>
        <p className="text-xs text-ink-muted">{t(`launcher.mcpFormatHints.${format}`)}</p>
        <pre className="overflow-x-auto whitespace-pre rounded bg-surface-1 px-3 py-2 font-mono text-[11.5px] text-foreground/90">{snippet(format, masked)}</pre>
      </div>
    </div>
  );
}

import { useCallback, useEffect, useState } from "react";
import { CodeEditor } from "@/components/code-editor";
import { useTranslation } from "react-i18next";
import { ChevronRight, File, FileText, Folder, FolderOpen, Loader2, RefreshCw, Save, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { showToast } from "@/components/toast";
import { errorText } from "@/lib/errors";
import { timeAgo } from "@/lib/time";
import { filesApi, serverApi, type FileEntry } from "./api";

/** Что открываем во встроенном редакторе; остальное — только в проводнике. */
const TEXT = /\.(properties|json|json5|ya?ml|toml|txt|cfg|conf|ini|log|mcmeta|sk|js|csv|xml|md|lang|snbt|sh|bat|cmd)$/i;

function size(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

/** Файлы сервера: папки, правка конфигов, удаление. */
export function ServerFiles({ id, running }: { id: string; running: boolean }) {
  const { t, i18n } = useTranslation();
  const [path, setPath] = useState<string[]>([]);
  const [entries, setEntries] = useState<FileEntry[] | null>(null);
  const [editing, setEditing] = useState<{ path: string; text: string; original: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirm, setConfirm] = useState<string | null>(null);

  const dir = path.join("/");
  const load = useCallback(() => {
    setEntries(null);
    filesApi.list(id, dir).then(setEntries, (e) => {
      showToast(errorText(e));
      setEntries([]);
    });
  }, [id, dir]);
  useEffect(load, [load]);

  const open = async (entry: FileEntry) => {
    const rel = [...path, entry.name].join("/");
    if (entry.dir) {
      setPath([...path, entry.name]);
      return;
    }
    try {
      const text = await filesApi.read(id, rel);
      setEditing({ path: rel, text, original: text });
    } catch (e) {
      showToast(errorText(e));
    }
  };

  const save = async () => {
    if (!editing) return;
    setSaving(true);
    try {
      await filesApi.write(id, editing.path, editing.text);
      setEditing({ ...editing, original: editing.text });
      showToast(t("files.saved"));
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  const remove = async (name: string) => {
    setConfirm(null);
    try {
      await filesApi.remove(id, [...path, name].join("/"));
      load();
    } catch (e) {
      showToast(errorText(e));
    }
  };

  if (editing) {
    const dirty = editing.text !== editing.original;
    return (
      <div className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <FileText className="size-4 text-ink-muted" />
          <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-foreground">{editing.path}</span>
          <Button size="sm" disabled={!dirty || saving} onClick={() => void save()}>
            {saving ? <Loader2 className="animate-spin" /> : <Save />} {t("files.save")}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => (!dirty || window.confirm(t("files.discard"))) && setEditing(null)}>
            <X /> {t("files.close")}
          </Button>
        </div>
        {running && <p className="text-xs text-ink-muted">{t("files.runningHint")}</p>}
        <CodeEditor
          value={editing.text}
          path={editing.path}
          onChange={(text) => setEditing({ ...editing, text })}
          onKeyDown={(e) => {
            if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
              e.preventDefault();
              if (dirty) void save();
            }
          }}
          className="h-[60vh]"
        />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-1 text-[13px]">
        <button type="button" onClick={() => setPath([])} className="rounded px-1.5 py-0.5 text-ink-muted outline-none hover:bg-surface-2 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">
          {t("files.root")}
        </button>
        {path.map((part, i) => (
          <span key={i} className="flex items-center gap-1">
            <ChevronRight className="size-3.5 text-ink-muted" />
            <button
              type="button"
              onClick={() => setPath(path.slice(0, i + 1))}
              className="rounded px-1.5 py-0.5 text-ink-muted outline-none hover:bg-surface-2 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring aria-[current=true]:text-foreground"
              aria-current={i === path.length - 1}
            >
              {part}
            </button>
          </span>
        ))}
        <span className="flex-1" />
        <Button variant="ghost" size="sm" onClick={load} aria-label={t("files.refresh")}>
          <RefreshCw />
        </Button>
        <Button variant="ghost" size="sm" onClick={() => void serverApi.openFolder(id, dir || undefined).catch((e) => showToast(errorText(e)))}>
          <FolderOpen /> {t("files.explorer")}
        </Button>
      </div>

      {entries === null ? (
        <Loader2 className="my-6 size-5 animate-spin text-ink-muted" />
      ) : entries.length === 0 ? (
        <p className="py-6 text-sm text-ink-muted">{t("files.empty")}</p>
      ) : (
        <ul className="flex flex-col divide-y divide-surface-3 rounded-lg border border-surface-3 bg-surface-1">
          {entries.map((e) => {
            const editable = e.dir || TEXT.test(e.name);
            return (
              <li key={e.name} className="group flex items-center gap-3 px-3 py-1.5">
                {e.dir ? <Folder className="size-4 shrink-0 text-[#e0951a]" /> : TEXT.test(e.name) ? <FileText className="size-4 shrink-0 text-ink-muted" /> : <File className="size-4 shrink-0 text-ink-muted" />}
                <button
                  type="button"
                  disabled={!editable}
                  onClick={() => void open(e)}
                  className="min-w-0 flex-1 truncate text-left text-[13px] text-foreground outline-none enabled:hover:underline focus-visible:underline disabled:cursor-default"
                >
                  {e.name}
                </button>
                <span className="hidden w-24 text-right text-xs text-ink-muted tabular-nums sm:block">{e.dir ? "" : size(e.size)}</span>
                <span className="hidden w-32 text-right text-xs text-ink-muted md:block">{e.modified ? timeAgo(new Date(e.modified * 1000).toISOString(), i18n.language) : ""}</span>
                {confirm === e.name ? (
                  <span className="flex items-center gap-1">
                    <Button size="sm" variant="ghost" onClick={() => void remove(e.name)} className="h-7 text-[#ff6b6b] hover:text-[#ff6b6b]">
                      {t("files.confirmDelete")}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setConfirm(null)} className="h-7">
                      {t("files.cancel")}
                    </Button>
                  </span>
                ) : (
                  <Button
                    size="icon"
                    variant="ghost"
                    disabled={running}
                    title={running ? t("hosting.stopToEdit") : t("files.delete")}
                    aria-label={t("files.delete")}
                    onClick={() => setConfirm(e.name)}
                    className="size-7 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                  >
                    <Trash2 />
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { showToast } from "@/components/toast";
import { ExportDialog } from "@/features/modpack/export-dialog";
import { errorText } from "@/lib/errors";
import { createStore } from "@/lib/store";
import { IconPickerDialog } from "./icon-picker";
import { OptionsDialog } from "./options-dialog";
import { SnapshotsDialog } from "./snapshots-dialog";
import { cloneInstance, deleteInstance, setInstanceMeta, useInstances } from "./store";
import { ProgressBar, type ByteProgress } from "@/components/ui/progress-bar";

type Kind = "delete" | "clone" | "export" | "icon" | "group" | "options" | "snapshots";

const store = createStore<{ kind: Kind; id: string } | null>(null);

/** Диалоги действий со сборкой — одни на всё приложение, открываются из меню и со страницы сборки. */
export const openInstanceDialog = (kind: Kind, id: string) => store.set({ kind, id });
const close = () => store.set(null);

export function InstanceDialogs() {
  const current = store.use();
  const instances = useInstances();
  const inst = instances.find((i) => i.id === current?.id);
  if (!current || !inst) return null;
  const onOpenChange = (open: boolean) => !open && close();

  switch (current.kind) {
    case "export":
      return <ExportDialog instance={inst} open onOpenChange={onOpenChange} />;
    case "icon":
      return <IconPickerDialog instance={inst} open onOpenChange={onOpenChange} />;
    case "delete":
      return <DeleteDialog id={inst.id} name={inst.name} />;
    case "clone":
      return <CloneDialog id={inst.id} name={inst.name} />;
    case "options":
      return <OptionsDialog id={inst.id} name={inst.name} onOpenChange={onOpenChange} />;
    case "snapshots":
      return <SnapshotsDialog id={inst.id} name={inst.name} onOpenChange={onOpenChange} />;
    case "group":
      return <GroupDialog id={inst.id} group={inst.group ?? ""} favorite={!!inst.favorite} />;
  }
}

function DeleteDialog({ id, name }: { id: string; name: string }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ByteProgress | null>(null);

  const remove = async () => {
    setBusy(true);
    setProgress(null);
    try {
      await deleteInstance(id, setProgress);
      close();
      if (location.pathname.includes(id)) navigate("/instances");
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && close()}>
      <DialogContent className="border-surface-3 bg-surface-1 sm:max-w-[420px]">
        <DialogHeader>
          <DialogTitle>{t("instance.deleteTitle", { name })}</DialogTitle>
          <DialogDescription>{t("instance.deleteText")}</DialogDescription>
        </DialogHeader>
        {busy && <ProgressBar unit="count" progress={progress} label={t("progress.deleting")} />}
        <DialogFooter>
          <Button variant="ghost" disabled={busy} onClick={close}>
            {t("create.cancel")}
          </Button>
          <Button variant="destructive" disabled={busy} onClick={() => void remove()}>
            {busy && <Loader2 className="animate-spin" />} {t("instance.deleteConfirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function CloneDialog({ id, name }: { id: string; name: string }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [value, setValue] = useState(t("clone.defaultName", { name }));
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ByteProgress | null>(null);

  const run = async () => {
    setBusy(true);
    setProgress(null);
    try {
      const copy = await cloneInstance(id, value, setProgress);
      close();
      showToast(t("clone.done", { name: copy.name }));
      navigate(`/instances/${copy.id}`);
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && !busy && close()}>
      <DialogContent className="border-surface-3 bg-surface-1 sm:max-w-[420px]">
        <DialogHeader>
          <DialogTitle>{t("clone.title", { name })}</DialogTitle>
          <DialogDescription>{t("clone.text")}</DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (value.trim()) void run();
          }}
          className="flex flex-col gap-4"
        >
          <Input autoFocus value={value} onChange={(e) => setValue(e.target.value)} maxLength={64} disabled={busy} aria-label={t("create.name")} />
          {busy && <ProgressBar progress={progress} label={t("clone.copying")} />}
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={close} disabled={busy}>
              {t("create.cancel")}
            </Button>
            <Button type="submit" disabled={busy || !value.trim()}>
              {busy && <Loader2 className="animate-spin" />} {t("clone.confirm")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function GroupDialog({ id, group, favorite }: { id: string; group: string; favorite: boolean }) {
  const { t } = useTranslation();
  const instances = useInstances();
  const groups = [...new Set(instances.flatMap((i) => (i.group ? [i.group] : [])))].sort();
  const [value, setValue] = useState(group);
  useEffect(() => setValue(group), [group]);

  const save = async (next: string | null) => {
    try {
      await setInstanceMeta(id, next, favorite);
      close();
    } catch (e) {
      showToast(errorText(e));
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && close()}>
      <DialogContent className="border-surface-3 bg-surface-1 sm:max-w-[400px]">
        <DialogHeader>
          <DialogTitle>{t("groups.moveTitle")}</DialogTitle>
          <DialogDescription>{t("groups.moveText")}</DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void save(value.trim() || null);
          }}
          className="flex flex-col gap-3"
        >
          <Input autoFocus list="shard-groups" value={value} onChange={(e) => setValue(e.target.value)} maxLength={40} placeholder={t("groups.placeholder")} />
          <datalist id="shard-groups">
            {groups.map((g) => (
              <option key={g} value={g} />
            ))}
          </datalist>
          {groups.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {groups.map((g) => (
                <button
                  key={g}
                  type="button"
                  onClick={() => void save(g)}
                  className="rounded-md bg-surface-2 px-2.5 py-1 text-[13px] outline-none hover:bg-surface-3 focus-visible:ring-2 focus-visible:ring-ring"
                >
                  {g}
                </button>
              ))}
            </div>
          )}
          <DialogFooter className="sm:justify-between">
            <Button type="button" variant="ghost" disabled={!group} onClick={() => void save(null)}>
              {t("groups.remove")}
            </Button>
            <Button type="submit">{t("groups.save")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

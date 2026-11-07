import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Loader2, RotateCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { showToast } from "@/components/toast";
import { backend, type JavaInfo } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";
import i18n from "@/i18n";
import { IconPickerDialog } from "./icon-picker";
import { InstanceIcon } from "./instance-icon";
import { LoaderVersionSelect } from "./loader-version-select";
import { updateInstance } from "./store";
import { defaultSettings, type Instance, type InstanceSettings } from "./types";

interface Form {
  name: string;
  memoryMb: number;
  settings: InstanceSettings;
  loaderVersion: string | null;
}

function formOf(inst: Instance): Form {
  return {
    name: inst.name,
    memoryMb: inst.memoryMb ?? 2048,
    settings: { ...defaultSettings, ...inst.settings },
    loaderVersion: inst.loaderVersion ?? null,
  };
}

export function InstanceSettingsForm({ instance, disabled }: { instance: Instance; disabled: boolean }) {
  const { t } = useTranslation();
  const [form, setForm] = useState(() => formOf(instance));
  const [saving, setSaving] = useState(false);
  const [totalMb, setTotalMb] = useState(8192);
  const [pickingIcon, setPickingIcon] = useState(false);

  useEffect(() => setForm(formOf(instance)), [instance]);
  useEffect(() => void backend.systemInfo().then((s) => setTotalMb(s.totalMemoryMb)), []);

  const dirty = useMemo(() => JSON.stringify(form) !== JSON.stringify(formOf(instance)), [form, instance]);
  const set = <K extends keyof InstanceSettings>(key: K, value: InstanceSettings[K]) =>
    setForm((f) => ({ ...f, settings: { ...f.settings, [key]: value } }));

  // Оставляем системе хотя бы 2 ГБ; шаг ползунка — 256 МБ
  const maxMemory = Math.max(1024, Math.floor((totalMb - 2048) / 256) * 256);
  const lowMemory = form.memoryMb < 2048;

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    setSaving(true);
    try {
      await updateInstance(instance.id, form);
    } catch (error) {
      showToast(errorText(error));
    } finally {
      setSaving(false);
    }
  };

  const numberOrNull = (v: string) => (v.trim() === "" ? null : Math.max(0, Math.floor(Number(v)) || 0));

  return (
    <form onSubmit={save} className="flex flex-col gap-4 pb-20">
      <Section title={t("instanceSettings.general")}>
        <Field label={t("icon.label")} htmlFor="inst-icon" hint={t("icon.hint")}>
          <Button
            id="inst-icon"
            type="button"
            variant="secondary"
            disabled={disabled}
            onClick={() => setPickingIcon(true)}
            className="h-11 gap-3 pl-1.5"
          >
            <InstanceIcon name={instance.name} loader={instance.loader} icon={instance.icon} className="size-8" />
            {t("icon.change")}
          </Button>
          <IconPickerDialog instance={instance} open={pickingIcon} onOpenChange={setPickingIcon} />
        </Field>
        <Field label={t("create.name")} htmlFor="inst-name">
          <Input
            id="inst-name"
            value={form.name}
            onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
            className="h-9 max-w-[420px] bg-surface-0"
          />
        </Field>
        {instance.loader && (
          <Field
            label={t("loaderVersion.label", { loader: t(`loader.${instance.loader}`) })}
            htmlFor="inst-loader-version"
            hint={t("loaderVersion.changeHint")}
          >
            <LoaderVersionSelect
              id="inst-loader-version"
              loader={instance.loader}
              mcVersion={instance.mcVersion}
              value={form.loaderVersion}
              onChange={(v) => setForm((f) => ({ ...f, loaderVersion: v }))}
              allowAuto={!instance.loaderVersion}
            />
          </Field>
        )}
      </Section>

      <Section title={t("instanceSettings.memory")} hint={t("instanceSettings.memoryHint", { total: gb(totalMb) })}>
        <div className="flex items-center gap-4">
          <input
            type="range"
            min={512}
            max={maxMemory}
            step={256}
            value={Math.min(form.memoryMb, maxMemory)}
            onChange={(e) => setForm((f) => ({ ...f, memoryMb: Number(e.target.value) }))}
            aria-label={t("instanceSettings.maxMemory")}
            className="h-1.5 flex-1 cursor-pointer accent-[var(--brand)]"
          />
          <output className="w-20 text-right font-display text-sm tabular-nums">{gb(form.memoryMb)}</output>
        </div>
        <MemoryPresets value={form.memoryMb} max={maxMemory} onChange={(mb) => setForm((f) => ({ ...f, memoryMb: mb }))} />
        {lowMemory && <p className="text-xs text-[#e0951a]">{t("instanceSettings.lowMemory")}</p>}
        <Field label={t("instanceSettings.minMemory")} htmlFor="inst-xms" hint={t("instanceSettings.minMemoryHint")}>
          <Input
            id="inst-xms"
            inputMode="numeric"
            value={form.settings.minMemoryMb ?? ""}
            placeholder="512"
            onChange={(e) => set("minMemoryMb", numberOrNull(e.target.value))}
            className="h-9 w-32 bg-surface-0 tabular-nums"
          />
        </Field>
      </Section>

      <Section title={t("instanceSettings.java")} hint={t("instanceSettings.javaHint")}>
        <JavaPicker value={form.settings.javaPath} onChange={(path) => set("javaPath", path)} />
      </Section>

      <Section title={t("instanceSettings.window")}>
        <div className="flex flex-wrap items-end gap-3">
          <Field label={t("instanceSettings.width")} htmlFor="inst-w">
            <Input
              id="inst-w"
              inputMode="numeric"
              placeholder="854"
              value={form.settings.width ?? ""}
              onChange={(e) => set("width", numberOrNull(e.target.value))}
              className="h-9 w-28 bg-surface-0 tabular-nums"
            />
          </Field>
          <span className="pb-2 text-ink-muted">×</span>
          <Field label={t("instanceSettings.height")} htmlFor="inst-h">
            <Input
              id="inst-h"
              inputMode="numeric"
              placeholder="480"
              value={form.settings.height ?? ""}
              onChange={(e) => set("height", numberOrNull(e.target.value))}
              className="h-9 w-28 bg-surface-0 tabular-nums"
            />
          </Field>
          <label className="flex h-9 items-center gap-2 pl-3 text-[13px]">
            <input
              type="checkbox"
              checked={form.settings.fullscreen}
              onChange={(e) => set("fullscreen", e.target.checked)}
              className="size-4 accent-[var(--brand)]"
            />
            {t("instanceSettings.fullscreen")}
          </label>
        </div>
        <p className="text-xs text-ink-muted">{t("instanceSettings.windowHint")}</p>
      </Section>

      <Section title={t("server.title")} hint={t("server.hint")}>
        <Field label={t("server.address")} htmlFor="inst-server">
          <Input
            id="inst-server"
            value={form.settings.server}
            placeholder="mc.example.net:25565"
            onChange={(e) => set("server", e.target.value)}
            className="h-9 max-w-[420px] bg-surface-0"
          />
        </Field>
      </Section>

      <Section title={t("instanceSettings.arguments")}>
        <Field label={t("instanceSettings.jvmArgs")} htmlFor="inst-jvm" hint={t("instanceSettings.jvmArgsHint")}>
          <Textarea id="inst-jvm" value={form.settings.jvmArgs} onChange={(v) => set("jvmArgs", v)} placeholder="-XX:+UseG1GC -XX:MaxGCPauseMillis=50" />
        </Field>
        <Field label={t("instanceSettings.gameArgs")} htmlFor="inst-game">
          <Textarea id="inst-game" value={form.settings.gameArgs} onChange={(v) => set("gameArgs", v)} placeholder="--quickPlaySingleplayer World" />
        </Field>
        <Field label={t("instanceSettings.env")} htmlFor="inst-env" hint={t("instanceSettings.envHint")}>
          <Textarea id="inst-env" value={form.settings.env} onChange={(v) => set("env", v)} placeholder="KEY=value" rows={3} />
        </Field>
      </Section>

      <Section title={t("instanceSettings.hooks")} hint={t("instanceSettings.hooksHint")}>
        <Field label={t("instanceSettings.preLaunch")} htmlFor="inst-pre">
          <Input
            id="inst-pre"
            value={form.settings.preLaunch}
            onChange={(e) => set("preLaunch", e.target.value)}
            spellCheck={false}
            className="h-9 bg-surface-0 font-mono text-[12px]"
          />
        </Field>
        <Field label={t("instanceSettings.postExit")} htmlFor="inst-post">
          <Input
            id="inst-post"
            value={form.settings.postExit}
            onChange={(e) => set("postExit", e.target.value)}
            spellCheck={false}
            className="h-9 bg-surface-0 font-mono text-[12px]"
          />
        </Field>
      </Section>

      {/* Панель сохранения появляется, только когда есть несохранённые изменения */}
      <div
        className={cn(
          "sticky bottom-4 flex items-center gap-3 self-end rounded-lg border border-surface-3 bg-surface-2 p-2 pl-4 shadow-lg transition-all",
          dirty ? "translate-y-0 opacity-100" : "pointer-events-none translate-y-2 opacity-0",
        )}
      >
        <span className="text-[13px] text-ink-muted">
          {disabled ? t("instanceSettings.appliesNextLaunch") : t("instanceSettings.unsaved")}
        </span>
        <Button type="button" variant="ghost" size="sm" onClick={() => setForm(formOf(instance))}>
          {t("instanceSettings.reset")}
        </Button>
        <Button type="submit" size="sm" disabled={saving || !form.name.trim()}>
          {saving ? <Loader2 className="animate-spin" /> : <Check />}
          {t("settings.save")}
        </Button>
      </div>
    </form>
  );
}

function gb(mb: number) {
  const value = (mb / 1024).toLocaleString(i18n.resolvedLanguage, { maximumFractionDigits: 1 });
  return i18n.t("instanceSettings.gb", { value });
}

function Section({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-3 rounded-lg border border-surface-3 bg-surface-1 px-5 py-4">
      <div>
        <h3 className="text-foreground">{title}</h3>
        {hint && <p className="text-sm text-ink-muted">{hint}</p>}
      </div>
      {children}
    </section>
  );
}

const PRESETS_GB = [4, 6, 8, 10, 12];

/** Быстрый выбор памяти: 4–12 ГБ одной кнопкой. Больше, чем есть в системе, не предлагаем. */
export function MemoryPresets({ value, max, onChange }: { value: number; max: number; onChange: (mb: number) => void }) {
  const { t } = useTranslation();
  return (
    <div role="radiogroup" aria-label={t("instanceSettings.memoryPresets")} className="flex flex-wrap gap-1.5">
      {PRESETS_GB.map((g) => {
        const mb = g * 1024;
        const active = value === mb;
        return (
          <button
            key={g}
            type="button"
            role="radio"
            aria-checked={active}
            disabled={mb > max}
            onClick={() => onChange(mb)}
            className={cn(
              "h-7 rounded-md border px-3 font-display text-xs tabular-nums outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-40",
              active ? "border-foreground bg-foreground text-background" : "border-surface-3 bg-surface-0 hover:border-ink-muted",
            )}
          >
            {g} GB
          </button>
        );
      })}
    </div>
  );
}

function Field({ label, htmlFor, hint, children }: { label: string; htmlFor: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={htmlFor} className="text-[13px] text-ink-muted">
        {label}
      </label>
      {children}
      {hint && <p className="text-xs text-ink-muted">{hint}</p>}
    </div>
  );
}

function Textarea({
  id,
  value,
  onChange,
  placeholder,
  rows = 2,
}: {
  id: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  rows?: number;
}) {
  return (
    <textarea
      id={id}
      rows={rows}
      value={value}
      placeholder={placeholder}
      spellCheck={false}
      onChange={(e) => onChange(e.target.value)}
      className="resize-y rounded-md border border-input bg-surface-0 px-3 py-2 font-mono text-[12px] outline-none placeholder:text-ink-muted/60 focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
    />
  );
}

/** Выбор Java: автоматически, одна из найденных или свой путь с проверкой. */
export function JavaPicker({ value, onChange }: { value: string | null; onChange: (path: string | null) => void }) {
  const { t } = useTranslation();
  const [found, setFound] = useState<JavaInfo[] | null>(null);
  const [custom, setCustom] = useState("");
  const [checking, setChecking] = useState(false);
  const [checked, setChecked] = useState<{ ok: true; info: JavaInfo } | { ok: false; error: string } | null>(null);

  const scan = () => {
    setFound(null);
    void backend.javaDetect().then(setFound);
  };
  useEffect(scan, []);

  const isFound = value !== null && found?.some((j) => j.path === value);
  const isCustom = value !== null && found !== null && !isFound;
  useEffect(() => {
    if (isCustom) setCustom(value ?? "");
  }, [isCustom, value]);

  const check = async () => {
    setChecking(true);
    try {
      const info = await backend.javaProbe(custom.trim());
      setChecked({ ok: true, info });
      onChange(info.path);
    } catch (e) {
      setChecked({ ok: false, error: errorText(e) });
    } finally {
      setChecking(false);
    }
  };

  const option = (active: boolean, onSelect: () => void, title: React.ReactNode, sub?: string, key?: string) => (
    <button
      key={key}
      type="button"
      role="radio"
      aria-checked={active}
      onClick={onSelect}
      className={cn(
        "flex items-center gap-3 rounded-md border px-3 py-2 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
        active ? "border-foreground bg-surface-2" : "border-surface-3 hover:border-ink-muted/60",
      )}
    >
      <span className={cn("grid size-4 shrink-0 place-items-center rounded-full border", active ? "border-foreground" : "border-ink-muted")}>
        {active && <span className="size-2 rounded-full bg-foreground" />}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[13px] text-foreground">{title}</span>
        {sub && <span className="block truncate text-xs text-ink-muted">{sub}</span>}
      </span>
    </button>
  );

  return (
    <div role="radiogroup" aria-label={t("instanceSettings.java")} className="flex flex-col gap-1.5">
      {option(value === null, () => onChange(null), t("instanceSettings.javaAuto"), t("instanceSettings.javaAutoHint"))}
      {found === null ? (
        <p className="flex items-center gap-2 px-1 py-1 text-xs text-ink-muted">
          <Loader2 className="size-3.5 animate-spin" /> {t("instanceSettings.javaScanning")}
        </p>
      ) : (
        found.map((j) =>
          option(
            value === j.path,
            () => onChange(j.path),
            `Java ${j.version}${j.managed ? ` ${t("instanceSettings.javaManaged")}` : ""}`,
            `${j.vendor ? `${j.vendor}, ` : ""}${j.path}`,
            j.path,
          ),
        )
      )}
      {option(isCustom || (value === null && custom !== "" && checked?.ok === false), () => setCustom(custom || " "), t("instanceSettings.javaCustom"))}
      {(isCustom || custom !== "") && (
        <div className="flex flex-col gap-1.5 pl-7">
          <div className="flex gap-2">
            <Input
              value={custom.trimStart()}
              onChange={(e) => {
                setCustom(e.target.value);
                setChecked(null);
              }}
              placeholder={t("instanceSettings.javaPathPlaceholder")}
              spellCheck={false}
              className="h-9 bg-surface-0 font-mono text-[12px]"
            />
            <Button type="button" variant="secondary" onClick={() => void check()} disabled={checking || !custom.trim()}>
              {checking ? <Loader2 className="animate-spin" /> : null}
              {t("instanceSettings.javaCheck")}
            </Button>
          </div>
          {checked && (
            <p className={cn("text-xs", checked.ok ? "text-ink-muted" : "text-destructive")}>
              {checked.ok ? t("instanceSettings.javaOk", { version: checked.info.version, arch: checked.info.arch }) : checked.error}
            </p>
          )}
        </div>
      )}
      <button type="button" onClick={scan} className="flex items-center gap-1.5 self-start px-1 py-1 text-xs text-ink-muted hover:text-foreground">
        <RotateCw className="size-3" /> {t("instanceSettings.javaRescan")}
      </button>
    </div>
  );
}

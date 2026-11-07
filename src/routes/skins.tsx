import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Ban,
  Check,
  FlipHorizontal2,
  Download,
  Gauge,
  Loader2,
  Pause,
  Play,
  Rotate3d,
  LogIn,
  Pencil,
  PersonStanding,
  Shirt,
  Trash2,
  Upload,
  UserSearch,
} from "lucide-react";
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Page } from "@/components/page";
import { showToast } from "@/components/toast";
import { currentAccount, openAccountsDialog, setOfflineSkin, useAccounts } from "@/features/accounts/store";
import {
  detectModel,
  exportSkin,
  loadImage,
  readFile,
  skinsApi,
  type Profile,
  type Skin,
} from "@/features/skins/api";
import { skinAnimations, SkinThumb, SkinViewer3D, type SkinAnimation } from "@/features/skins/skin-viewer";
import { FiguraDetails, FiguraLibrary, useFigura } from "@/features/figura/figura-panel";
import { FiguraViewer } from "@/features/figura/figura-viewer";
import { CpmDetails, CpmLibrary, useCpm } from "@/features/cpm/cpm-panel";
import { CapeThumb } from "@/features/skins/cape-thumb";
import { setAccountSkin } from "@/features/accounts/account-face";
import { cachedProfile, saveProfile } from "@/features/skins/api";
import { useSearchParams } from "react-router";
import { isTauri } from "@/lib/backend";
import { errorText } from "@/lib/errors";
import { cn } from "@/lib/utils";

const wardrobes = ["minecraft", "figura", "cpm"] as const;
type Wardrobe = (typeof wardrobes)[number];
const speeds = [0.5, 1, 2];

export function SkinsPage() {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const account = currentAccount(accounts);
  const microsoft = account?.kind === "microsoft" ? account : null;
  const offline = account?.kind === "offline" ? account : null;
  // В превью интерфейса без Rust профиль понарошку — чтобы были видны плащи
  const profileAccount = microsoft?.id ?? (isTauri ? null : "preview");

  const [params] = useSearchParams();
  const [tab, setTab] = useState<Wardrobe>(() => wardrobes.find((w) => w === params.get("tab")) ?? "minecraft");
  const figura = useFigura();
  const cpm = useCpm();

  const [skins, setSkins] = useState<Skin[] | null>(null);
  const [data, setData] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [animation, setAnimation] = useState<SkinAnimation>("idle");
  const [figuraAnimations, setFiguraAnimations] = useState<string[]>([]);
  const [figuraAnimation, setFiguraAnimation] = useState<string | null>(null);
  const [speed, setSpeed] = useState(1);
  const [paused, setPaused] = useState(false);
  const [autoRotate, setAutoRotate] = useState(false);
  const [elytra, setElytra] = useState(false);
  const [view, setView] = useState<"front" | "back">("front");
  const [busy, setBusy] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<Skin | null>(null);
  const [byName, setByName] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  // Профиль Microsoft-аккаунта: что надето сейчас и какие есть плащи
  const [profile, setProfile] = useState<Profile | null>(null);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [textures, setTextures] = useState<Record<string, string>>({});

  const refresh = async () => {
    const list = await skinsApi.list();
    setSkins(list);
    setSelected((s) => (s && list.some((x) => x.id === s) ? s : (list[0]?.id ?? null)));
    const missing = list.filter((s) => !data[s.id]);
    const loaded = await Promise.all(missing.map(async (s) => [s.id, await skinsApi.data(s.id)] as const));
    if (loaded.length) setData((d) => ({ ...d, ...Object.fromEntries(loaded) }));
  };

  useEffect(() => {
    refresh().catch((e) => showToast(errorText(e)));
  }, []);

  useEffect(() => {
    setFiguraAnimations([]);
    setFiguraAnimation(null);
  }, [figura.avatar?.id]);

  const texture = async (url: string) => {
    if (textures[url]) return textures[url];
    const src = await skinsApi.texture(url);
    setTextures((x) => ({ ...x, [url]: src }));
    return src;
  };

  const applyProfile = (p: Profile) => {
    setProfile(p);
    for (const item of [...p.skins, ...p.capes]) void texture(item.url).catch(() => {});
  };

  useEffect(() => {
    setProfileError(null);
    // Сохранённый профиль показываем сразу, свежий подтягиваем следом
    const cached = profileAccount ? cachedProfile(profileAccount) : null;
    setProfile(cached?.profile ?? null);
    if (cached) setTextures((x) => ({ ...cached.textures, ...x }));
    if (!profileAccount) return;
    skinsApi.profile(profileAccount).then(applyProfile, (e) => setProfileError(errorText(e)));
  }, [profileAccount]);

  // Профиль и его текстуры — в кэш аккаунта, а лицо — в голову в меню
  useEffect(() => {
    if (!profileAccount || !profile) return;
    const urls = [...profile.skins, ...profile.capes].map((i) => i.url);
    if (!urls.every((u) => textures[u])) return;
    saveProfile(profileAccount, profile, Object.fromEntries(urls.map((u) => [u, textures[u]])));
    const active = profile.skins.find((x) => x.state === "ACTIVE");
    if (microsoft && active) void setAccountSkin(microsoft.uuid, textures[active.url]);
  }, [profile, textures]);

  const skin = skins?.find((s) => s.id === selected) ?? null;
  const activeSkin = profile?.skins.find((s) => s.state === "ACTIVE") ?? null;
  const activeCape = profile?.capes.find((c) => c.state === "ACTIVE") ?? null;
  const [capePreview, setCapePreview] = useState<string | null | undefined>(undefined);
  // Пока плащ не выбран вручную, показываем надетый
  const shownCape = capePreview === undefined ? activeCape?.id ?? null : capePreview;
  const capeUrl = profile?.capes.find((c) => c.id === shownCape)?.url;
  const capePending = profile !== null && shownCape !== (activeCape?.id ?? null);

  const addFiles = async (files: FileList | null) => {
    for (const file of Array.from(files ?? [])) {
      try {
        const src = await readFile(file);
        const img = await loadImage(src);
        const added = await skinsApi.addData(src, file.name.replace(/\.png$/i, ""), detectModel(img));
        setSelected(added.id);
      } catch (e) {
        showToast(errorText(e));
      }
    }
    await refresh();
  };

  const run = async (key: string, fn: () => Promise<unknown>) => {
    setBusy(key);
    try {
      await fn();
    } catch (e) {
      showToast(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const wear = (s: Skin) =>
    run(`wear:${s.id}`, async () => {
      if (offline) {
        await setOfflineSkin(offline.id, s.id);
        if (data[s.id]) void setAccountSkin(offline.uuid, data[s.id]);
        showToast(t("skins.appliedOffline", { name: s.name }));
        return;
      }
      applyProfile(await skinsApi.apply(microsoft!.id, s.id));
      showToast(t("skins.applied", { name: s.name }));
    });

  const takeOff = () =>
    run("wear:none", async () => {
      await setOfflineSkin(offline!.id, null);
      showToast(t("skins.removedOffline"));
    });

  const setCape = (id: string | null) =>
    run(`cape:${id}`, async () => {
      applyProfile(await skinsApi.setCape(profileAccount!, id));
      setCapePreview(undefined);
      showToast(t(id ? "skins.capeApplied" : "skins.capeRemoved"));
    });

  const saveActive = () =>
    run("save-active", async () => {
      if (!activeSkin) return;
      const src = await texture(activeSkin.url);
      const added = await skinsApi.addData(src, profile!.name, activeSkin.variant?.toLowerCase() === "slim" ? "slim" : "classic");
      await refresh();
      setSelected(added.id);
    });

  const remove = (s: Skin) =>
    run(`remove:${s.id}`, async () => {
      await skinsApi.remove(s.id);
      await refresh();
    });

  const activeInLibrary = useMemo(
    () => !!activeSkin && !!skins?.some((s) => data[s.id] && textures[activeSkin.url] === data[s.id]),
    [activeSkin, skins, data, textures],
  );

  // Скин в превью: выбранный в библиотеке или надетый на аккаунт. Его же видно и под моделью CPM.
  const shownSkin = skin ? (data[skin.id] ?? null) : activeSkin ? (textures[activeSkin.url] ?? null) : null;
  const shownModel = skin?.model ?? (activeSkin?.variant?.toLowerCase() === "slim" ? "slim" : "classic");
  const isFigura = tab === "figura";

  const capeWearAction = (
    <div className="flex flex-wrap items-center gap-2">
      <Button size="sm" disabled={busy !== null} onClick={() => void setCape(shownCape)}>
        {busy === `cape:${shownCape}` ? <Loader2 className="animate-spin" /> : <Check />}
        {shownCape
          ? t("skins.wearCape", { name: profile?.capes.find((c) => c.id === shownCape)?.alias ?? t("skins.cape") })
          : t("skins.takeOffCape")}
      </Button>
      <Button size="sm" variant="ghost" onClick={() => setCapePreview(undefined)}>
        {t("create.cancel")}
      </Button>
    </div>
  );

  return (
    <Page title={t("skins.title")}>
      {tab === "minecraft" && offline && (
        <div className="flex flex-wrap items-center gap-3 rounded-lg border border-surface-3 bg-surface-1 px-4 py-3 text-[13px] text-ink-muted">
          <Shirt className="size-4 shrink-0" />
          <p className="min-w-0 flex-1">
            {t("skins.offlineSkinHint", { name: offline.name })}
            {offline.skin && skins?.some((s) => s.id === offline.skin) && (
              <span className="block text-foreground">{t("skins.offlineWearing", { name: skins.find((s) => s.id === offline.skin)!.name })}</span>
            )}
          </p>
          {offline.skin && (
            <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void takeOff()}>
              {t("skins.takeOff")}
            </Button>
          )}
        </div>
      )}
      {tab === "minecraft" && !microsoft && !offline && (
        <div className="flex flex-wrap items-center gap-3 rounded-lg border border-surface-3 bg-surface-1 px-4 py-3 text-[13px] text-ink-muted">
          <Shirt className="size-4 shrink-0" />
          <p className="min-w-0 flex-1">{t("skins.offlineHint")}</p>
          <Button size="sm" variant="secondary" onClick={openAccountsDialog}>
            <LogIn /> {t("skins.signIn")}
          </Button>
        </div>
      )}
      {tab === "minecraft" && profileError && (
        <p className="rounded-lg border border-[#e0951a]/30 bg-[#e0951a]/8 px-4 py-3 text-[13px] text-[#e0951a]">
          {t("skins.profileFailed", { error: profileError })}
        </p>
      )}

      <div className="grid gap-5 md:grid-cols-[minmax(280px,340px)_1fr]">
        {/* Единый предпросмотр: скин, аватар Figura или скин под моделью CPM */}
        <section className="flex flex-col gap-3 rounded-xl border border-surface-3 bg-surface-1 p-4 md:sticky md:top-4 md:self-start">
          <div className="relative h-[360px] overflow-hidden rounded-lg bg-surface-0">
            {isFigura ? (
              figura.files && figura.files.models.length > 0 ? (
                <FiguraViewer
                  models={figura.files.models}
                  textures={figura.files.textures}
                  animation={figuraAnimation}
                  playing={!paused}
                  speed={speed}
                  onAnimations={(names) => {
                    setFiguraAnimations(names);
                    setFiguraAnimation((a) => a ?? names[0] ?? null);
                  }}
                  className="absolute inset-0"
                />
              ) : (
                <div className="grid h-full place-items-center px-6 text-center text-[13px] text-ink-muted">
                  {!figura.avatar ? t("skins.nothingSelected") : figura.files ? t("figura.noModel") : <Loader2 className="size-5 animate-spin" />}
                </div>
              )
            ) : (
              <SkinViewer3D
                skin={shownSkin}
                model={shownModel}
                cape={tab === "minecraft" && capeUrl ? (textures[capeUrl] ?? null) : null}
                animation={animation}
                speed={speed}
                paused={paused}
                autoRotate={autoRotate}
                elytra={elytra}
                view={view}
                className="absolute inset-0"
              />
            )}

            <div className="absolute top-2 left-2 flex gap-1.5">
              {tab === "minecraft" && (
                <CapeMenu
                  profile={profile}
                  textures={textures}
                  shownCape={shownCape}
                  wornCape={activeCape?.id ?? null}
                  onPick={(id) => {
                    setCapePreview(id);
                    if (id) setView("back");
                  }}
                  footer={capePending ? capeWearAction : null}
                  disabledHint={!profileAccount ? t("skins.needMicrosoft") : null}
                />
              )}
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button size="sm" variant="secondary" className="h-7 bg-surface-2/90 px-2.5 text-xs backdrop-blur" disabled={isFigura && figuraAnimations.length === 0}>
                    <PersonStanding /> {t("skins.poses")}
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="max-h-80 w-48 overflow-y-auto border-surface-3 bg-surface-1">
                  <DropdownMenuLabel className="text-xs text-ink-muted">{t("skins.animation")}</DropdownMenuLabel>
                  {isFigura ? (
                    <DropdownMenuRadioGroup value={figuraAnimation ?? ""} onValueChange={(v) => setFiguraAnimation(v || null)}>
                      <DropdownMenuRadioItem value="">{t("figura.still")}</DropdownMenuRadioItem>
                      {figuraAnimations.map((a) => (
                        <DropdownMenuRadioItem key={a} value={a} className="truncate">
                          {a}
                        </DropdownMenuRadioItem>
                      ))}
                    </DropdownMenuRadioGroup>
                  ) : (
                    <DropdownMenuRadioGroup
                      value={animation}
                      onValueChange={(v) => {
                        setAnimation(v as SkinAnimation);
                        setPaused(false);
                      }}
                    >
                      {skinAnimations.map((a) => (
                        <DropdownMenuRadioItem key={a} value={a}>
                          {t(`skins.animations.${a}`)}
                        </DropdownMenuRadioItem>
                      ))}
                    </DropdownMenuRadioGroup>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
            </div>

            <div className="absolute right-2 bottom-2 left-2 flex items-center gap-1">
              <Button
                size="icon"
                variant="ghost"
                className="size-7"
                disabled={isFigura ? !figuraAnimation : animation === "none"}
                title={paused ? t("figura.play") : t("figura.pause")}
                aria-label={paused ? t("figura.play") : t("figura.pause")}
                onClick={() => setPaused((p) => !p)}
              >
                {paused ? <Play /> : <Pause />}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="h-7 gap-1 px-2 text-xs tabular-nums"
                title={t("figura.speed")}
                onClick={() => setSpeed((v) => speeds[(speeds.indexOf(v) + 1) % speeds.length])}
              >
                <Gauge className="size-3.5" /> {speed}×
              </Button>
              {!isFigura && (
                <>
                  <Button
                    size="icon"
                    variant={autoRotate ? "secondary" : "ghost"}
                    className="size-7"
                    title={t("skins.autoRotate")}
                    aria-label={t("skins.autoRotate")}
                    aria-pressed={autoRotate}
                    onClick={() => setAutoRotate((v) => !v)}
                  >
                    <Rotate3d />
                  </Button>
                  {tab === "minecraft" && capeUrl && (
                    <div role="radiogroup" aria-label={t("skins.backEquipment")} className="ml-auto flex rounded-md bg-surface-2 p-0.5">
                      {([false, true] as const).map((wings) => (
                        <button
                          key={String(wings)}
                          type="button"
                          role="radio"
                          aria-checked={elytra === wings}
                          onClick={() => {
                            setElytra(wings);
                            setView("back");
                          }}
                          className={cn(
                            "h-6 rounded px-2 text-[11px] text-ink-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                            elytra === wings && "bg-surface-0 text-foreground",
                          )}
                        >
                          {wings ? t("skins.elytra") : t("skins.cape")}
                        </button>
                      ))}
                    </div>
                  )}
                  <Button
                    size="icon"
                    variant="ghost"
                    className={cn("size-7", !(tab === "minecraft" && capeUrl) && "ml-auto")}
                    title={view === "back" ? t("skins.viewFront") : t("skins.viewBack")}
                    aria-label={view === "back" ? t("skins.viewFront") : t("skins.viewBack")}
                    onClick={() => setView((v) => (v === "back" ? "front" : "back"))}
                  >
                    <FlipHorizontal2 />
                  </Button>
                </>
              )}
            </div>
          </div>

          {tab === "minecraft" && (
            <>
              <p className="min-w-0 truncate font-display text-[15px]">{skin?.name ?? t("skins.onAccount")}</p>
              {capePending && capeWearAction}
              {skin && (
                <div className="flex gap-2">
                  <Button
                    className="flex-1"
                    disabled={(!microsoft && !offline) || busy !== null || (!!offline && offline.skin === skin.id)}
                    title={!microsoft && !offline ? t("skins.needMicrosoft") : undefined}
                    onClick={() => void wear(skin)}
                  >
                    {busy === `wear:${skin.id}` ? <Loader2 className="animate-spin" /> : <Shirt />}
                    {t("skins.wear")}
                  </Button>
                  <Button size="icon" variant="ghost" title={t("skins.rename")} aria-label={t("skins.rename")} onClick={() => setRenaming(skin)}>
                    <Pencil />
                  </Button>
                  <Button
                    size="icon"
                    variant="ghost"
                    title={t("skins.export")}
                    aria-label={t("skins.export")}
                    onClick={() => void exportSkin(skin, t("skins.export")).catch((e) => showToast(errorText(e)))}
                  >
                    <Download />
                  </Button>
                  <Button
                    size="icon"
                    variant="ghost"
                    title={t("skins.remove")}
                    aria-label={t("skins.remove")}
                    disabled={busy !== null}
                    onClick={() => void remove(skin)}
                    className="hover:text-destructive"
                  >
                    <Trash2 />
                  </Button>
                </div>
              )}
            </>
          )}
          {tab === "figura" && <FiguraDetails figura={figura} />}
          {tab === "cpm" && (
            <>
              <p className="text-xs text-ink-muted">{t("cpm.previewNote")}</p>
              <CpmDetails cpm={cpm} />
            </>
          )}
        </section>

        <div className="flex min-w-0 flex-col gap-4">
          <div role="tablist" aria-label={t("skins.wardrobe")} className="flex gap-1 border-b border-surface-3">
            {wardrobes.map((key) => (
              <button
                key={key}
                type="button"
                role="tab"
                aria-selected={tab === key}
                onClick={() => setTab(key)}
                className={cn(
                  "-mb-px border-b-2 px-3 py-2 text-[13px] outline-none transition-colors focus-visible:text-foreground",
                  tab === key ? "border-foreground text-foreground" : "border-transparent text-ink-muted hover:text-foreground",
                )}
              >
                {t(`skins.wardrobes.${key}`)}
              </button>
            ))}
          </div>

          {tab === "minecraft" && (
            <>
              {profileAccount && profile && (
                <div className="flex flex-wrap items-center gap-3 rounded-lg border border-surface-3 bg-surface-1 p-3">
                  {activeSkin && (
                    <button
                      type="button"
                      onClick={() => setSelected(null)}
                      className={cn(
                        "grid h-20 w-14 place-items-center rounded-md bg-surface-0 outline-none focus-visible:ring-2 focus-visible:ring-ring",
                        selected === null && "ring-2 ring-brand",
                      )}
                      title={t("skins.onAccount")}
                    >
                      <SkinThumb
                        src={textures[activeSkin.url] ?? null}
                        model={activeSkin.variant?.toLowerCase() === "slim" ? "slim" : "classic"}
                        className="h-16 w-8"
                      />
                    </button>
                  )}
                  <div className="min-w-0 flex-1">
                    <p className="text-[13px]">{t("skins.accountTitle", { name: profile.name })}</p>
                    <p className="text-xs text-ink-muted">{t("skins.onAccountHint")}</p>
                  </div>
                  {activeSkin && !activeInLibrary && (
                    <Button size="sm" variant="secondary" disabled={busy !== null} onClick={() => void saveActive()}>
                      {busy === "save-active" ? <Loader2 className="animate-spin" /> : <Download />}
                      {t("skins.saveToLibrary")}
                    </Button>
                  )}
                </div>
              )}

              <div className="flex flex-wrap items-center gap-2">
                <h2 className="min-w-0 flex-1 text-sm text-ink-muted">{t("skins.library", { count: skins?.length ?? 0 })}</h2>
                <Button size="sm" variant="ghost" onClick={() => setByName(true)}>
                  <UserSearch /> {t("skins.byName")}
                </Button>
                <Button size="sm" onClick={() => fileInput.current?.click()}>
                  <Upload /> {t("skins.addFile")}
                </Button>
                <input
                  ref={fileInput}
                  type="file"
                  accept="image/png"
                  multiple
                  className="hidden"
                  onChange={(e) => {
                    void addFiles(e.target.files);
                    e.target.value = "";
                  }}
                />
              </div>

              {skins === null ? (
                <div className="grid h-40 place-items-center text-ink-muted">
                  <Loader2 className="size-5 animate-spin" />
                </div>
              ) : skins.length === 0 ? (
                <p className="rounded-lg border border-dashed border-surface-3 px-4 py-8 text-center text-[13px] text-ink-muted">
                  {t("skins.emptyText")}
                </p>
              ) : (
                <ul className="grid grid-cols-[repeat(auto-fill,minmax(112px,1fr))] gap-2">
                  {skins.map((s) => (
                    <li key={s.id}>
                      <button
                        type="button"
                        onClick={() => setSelected(s.id)}
                        onDoubleClick={() => (microsoft || offline) && void wear(s)}
                        className={cn(
                          "flex w-full flex-col items-center gap-2 rounded-lg border bg-surface-1 px-2 pt-3 pb-2 outline-none transition-colors hover:border-ink-muted focus-visible:ring-2 focus-visible:ring-ring",
                          selected === s.id ? "border-brand" : "border-surface-3",
                        )}
                      >
                        <SkinThumb src={data[s.id] ?? null} model={s.model} className="h-24 w-12" />
                        <span className="w-full truncate text-center text-xs">
                          {offline?.skin === s.id && <Check className="mr-1 inline size-3 text-brand" />}
                          {s.name}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </>
          )}
          {tab === "figura" && <FiguraLibrary figura={figura} />}
          {tab === "cpm" && <CpmLibrary cpm={cpm} />}
        </div>
      </div>

      <RenameDialog
        skin={renaming}
        onClose={() => setRenaming(null)}
        onSave={(s, name) =>
          run(`rename:${s.id}`, async () => {
            await skinsApi.update(s.id, name, s.model);
            await refresh();
            setRenaming(null);
          })
        }
      />
      <ByNameDialog
        open={byName}
        onOpenChange={setByName}
        onAdded={async (s) => {
          await refresh();
          setSelected(s.id);
        }}
      />
    </Page>
  );
}

/** Кнопка «Плащи» над превью: плитки плащей аккаунта, нажатие — примерка. */
function CapeMenu({
  profile,
  textures,
  shownCape,
  wornCape,
  onPick,
  footer,
  disabledHint,
}: {
  profile: Profile | null;
  textures: Record<string, string>;
  shownCape: string | null;
  wornCape: string | null;
  onPick: (id: string | null) => void;
  footer: React.ReactNode;
  disabledHint: string | null;
}) {
  const { t } = useTranslation();
  const capes = profile?.capes ?? [];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          size="sm"
          variant="secondary"
          className="h-7 bg-surface-2/90 px-2.5 text-xs backdrop-blur"
          disabled={!!disabledHint}
          title={disabledHint ?? undefined}
        >
          <Shirt /> {t("skins.capes")}
          {capes.length > 0 && <span className="text-ink-muted tabular-nums">{capes.length}</span>}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-[300px] border-surface-3 bg-surface-1 p-2">
        <DropdownMenuLabel className="px-1 text-xs text-ink-muted">
          {capes.length ? t("skins.capesHint") : t("skins.noCapes")}
        </DropdownMenuLabel>
        {capes.length > 0 && (
          <ul role="radiogroup" aria-label={t("skins.cape")} className="grid max-h-72 grid-cols-3 gap-1.5 overflow-y-auto p-1">
            {[null, ...capes].map((cape) => {
              const id = cape?.id ?? null;
              const name = cape ? (cape.alias ?? t("skins.cape")) : t("skins.noCape");
              return (
                <li key={id ?? "none"}>
                  <button
                    type="button"
                    role="radio"
                    aria-checked={shownCape === id}
                    title={name}
                    onClick={() => onPick(id)}
                    className={cn(
                      "relative flex w-full flex-col items-center gap-1 rounded-md border-2 bg-surface-0 px-1 pt-2 pb-1 outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
                      shownCape === id ? "border-brand" : "border-transparent hover:border-surface-3",
                    )}
                  >
                    {cape ? (
                      <CapeThumb src={textures[cape.url] ?? null} className="h-12 w-[60px]" />
                    ) : (
                      <span className="grid h-12 w-[60px] place-items-center text-ink-muted">
                        <Ban className="size-5" />
                      </span>
                    )}
                    <span className="w-full truncate text-center text-[11px] text-ink">{name}</span>
                    {wornCape === id && (
                      <span className="absolute top-1 right-1 grid size-4 place-items-center rounded-full bg-brand text-brand-foreground">
                        <Check className="size-3" />
                      </span>
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
        {footer && <div className="border-t border-surface-3 px-1 pt-2">{footer}</div>}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function RenameDialog({
  skin,
  onClose,
  onSave,
}: {
  skin: Skin | null;
  onClose: () => void;
  onSave: (skin: Skin, name: string) => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  useEffect(() => setName(skin?.name ?? ""), [skin]);
  return (
    <Dialog open={!!skin} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="border-surface-3 bg-surface-1 sm:max-w-[380px]">
        <DialogHeader>
          <DialogTitle>{t("skins.rename")}</DialogTitle>
        </DialogHeader>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (skin && name.trim()) onSave(skin, name);
          }}
        >
          <Input value={name} onChange={(e) => setName(e.target.value)} maxLength={64} autoFocus className="bg-surface-0" />
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={onClose}>
              {t("create.cancel")}
            </Button>
            <Button type="submit" disabled={!name.trim()}>
              {t("skins.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function ByNameDialog({
  open,
  onOpenChange,
  onAdded,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAdded: (skin: Skin) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setName("");
      setError(null);
    }
  }, [open]);

  const submit = async () => {
    setLoading(true);
    setError(null);
    try {
      await onAdded(await skinsApi.addPlayer(name));
      onOpenChange(false);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="border-surface-3 bg-surface-1 sm:max-w-[420px]">
        <DialogHeader>
          <DialogTitle>{t("skins.byNameTitle")}</DialogTitle>
          <DialogDescription>{t("skins.byNameText")}</DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (name.trim()) void submit();
          }}
        >
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("skins.byNamePlaceholder")}
            maxLength={16}
            autoFocus
            className="bg-surface-0"
          />
          {error && (
            <p role="alert" className="text-[13px] text-destructive">
              {error}
            </p>
          )}
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              {t("create.cancel")}
            </Button>
            <Button type="submit" disabled={!name.trim() || loading}>
              {loading && <Loader2 className="animate-spin" />}
              {t("skins.add")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

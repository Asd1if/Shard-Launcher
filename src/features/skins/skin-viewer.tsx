import { useEffect, useRef, useState } from "react";
import {
  CrouchAnimation,
  FlyingAnimation,
  FunctionAnimation,
  HitAnimation,
  IdleAnimation,
  RunningAnimation,
  SkinViewer,
  SwimAnimation,
  WalkingAnimation,
  WaveAnimation,
  type PlayerAnimation,
} from "skinview3d";
import { cn } from "@/lib/utils";
import { loadImage, type SkinModel } from "./api";

export const skinAnimations = ["idle", "walk", "run", "fly", "wave", "crouch", "hit", "swim", "dance", "sit", "none"] as const;
export type SkinAnimation = (typeof skinAnimations)[number];

/** Танец: руки по очереди вверх, покачивание корпуса и головы. */
const dance = () =>
  new FunctionAnimation((player, t) => {
    const s = player.skin;
    const beat = Math.sin(t * 6);
    s.rightArm.rotation.x = 0;
    s.leftArm.rotation.x = 0;
    s.rightArm.rotation.z = Math.PI * 0.75 * Math.max(0, beat) + 0.1;
    s.leftArm.rotation.z = -Math.PI * 0.75 * Math.max(0, -beat) - 0.1;
    s.body.rotation.y = Math.sin(t * 3) * 0.25;
    s.head.rotation.x = Math.abs(Math.sin(t * 6)) * 0.25;
    s.head.rotation.y = Math.sin(t * 3) * 0.35;
    s.rightLeg.rotation.x = Math.max(0, beat) * 0.5;
    s.leftLeg.rotation.x = Math.max(0, -beat) * 0.5;
    player.position.y = Math.abs(beat) * 1.2;
  });

/** Сидит, как на лодке или вагонетке; голова оглядывается. */
const sit = () =>
  new FunctionAnimation((player, t) => {
    const s = player.skin;
    s.rightLeg.rotation.x = -Math.PI / 2;
    s.leftLeg.rotation.x = -Math.PI / 2;
    s.rightLeg.rotation.z = 0.15;
    s.leftLeg.rotation.z = -0.15;
    s.rightArm.rotation.x = -0.6;
    s.leftArm.rotation.x = -0.6;
    s.head.rotation.y = Math.sin(t * 0.8) * 0.6;
    player.position.y = -6;
  });

function makeAnimation(kind: SkinAnimation): PlayerAnimation | null {
  switch (kind) {
    case "idle":
      return new IdleAnimation();
    case "walk":
      return new WalkingAnimation();
    case "run":
      return new RunningAnimation();
    case "fly":
      return new FlyingAnimation();
    case "wave":
      return new WaveAnimation();
    case "crouch":
      return new CrouchAnimation();
    case "hit":
      return new HitAnimation();
    case "swim":
      return new SwimAnimation();
    case "dance":
      return dance();
    case "sit":
      return sit();
    case "none":
      return null;
  }
}

/** 3D-модель игрока: вращается мышью, колесо — приближение. */
export function SkinViewer3D({
  skin,
  model,
  cape,
  animation,
  speed = 1,
  paused = false,
  autoRotate = false,
  elytra = false,
  view = "front",
  className,
}: {
  /** data URL скина; null — пустая модель */
  skin: string | null;
  model: SkinModel;
  cape: string | null;
  animation: SkinAnimation;
  speed?: number;
  paused?: boolean;
  autoRotate?: boolean;
  /** Плащ в виде элитр */
  elytra?: boolean;
  /** Сторона, к которой повёрнута камера: сзади видно плащ и элитры */
  view?: "front" | "back";
  className?: string;
}) {
  const box = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const viewer = useRef<SkinViewer | null>(null);

  useEffect(() => {
    const el = box.current!;
    const v = new SkinViewer({ canvas: canvas.current!, width: el.clientWidth, height: el.clientHeight, zoom: 0.75 });
    v.background = null;
    v.fov = 40;
    v.camera.position.set(-18, 8, 42);
    v.controls.enablePan = false;
    viewer.current = v;
    const resize = new ResizeObserver(() => v.setSize(el.clientWidth, el.clientHeight));
    resize.observe(el);
    return () => {
      resize.disconnect();
      v.dispose();
      viewer.current = null;
    };
  }, []);

  useEffect(() => {
    const v = viewer.current;
    if (!v) return;
    if (skin) void v.loadSkin(skin, { model: model === "slim" ? "slim" : "default" });
    else v.loadSkin(null);
  }, [skin, model]);

  // Элитры рисуются текстурой плаща, поэтому без плаща их не видно и в полёте
  const flying = animation === "fly";
  useEffect(() => {
    const v = viewer.current;
    if (!v) return;
    if (cape) void v.loadCape(cape, { backEquipment: elytra || flying ? "elytra" : "cape" });
    else v.loadCape(null);
  }, [cape, elytra, flying]);

  useEffect(() => {
    const v = viewer.current;
    if (!v) return;
    v.playerObject.position.y = 0;
    v.playerObject.skin.resetJoints();
    const a = makeAnimation(animation);
    if (a) a.speed = speed;
    v.animation = a;
  }, [animation]);

  useEffect(() => {
    const v = viewer.current;
    if (!v?.animation) return;
    v.animation.speed = speed;
    v.animation.paused = paused;
  }, [speed, paused, animation]);

  useEffect(() => {
    const v = viewer.current;
    if (!v) return;
    v.camera.position.set(view === "back" ? 18 : -18, 8, view === "back" ? -42 : 42);
    // Направление задаём сами, а расстояние — по zoom, чтобы игрок помещался целиком
    v.adjustCameraDistance();
    v.controls.update();
  }, [view]);

  useEffect(() => {
    const v = viewer.current;
    if (!v) return;
    v.autoRotate = autoRotate;
    v.autoRotateSpeed = 1.2;
  }, [autoRotate]);

  return (
    <div ref={box} className={cn("relative cursor-grab active:cursor-grabbing", className)}>
      <canvas ref={canvas} className="absolute inset-0 outline-none" />
    </div>
  );
}

// Фронтальная развёртка скина: [x, y, w, h] на текстуре → место на картинке 16×32.
type Part = [sx: number, sy: number, w: number, h: number, dx: number, dy: number];

function frontParts(model: SkinModel, legacy: boolean): Part[] {
  const arm = model === "slim" ? 3 : 4;
  const base: Part[] = [
    [8, 8, 8, 8, 4, 0], // голова
    [20, 20, 8, 12, 4, 8], // туловище
    [44, 20, arm, 12, 4 - arm, 8], // правая рука
    [4, 20, 4, 12, 4, 20], // правая нога
  ];
  if (!legacy) {
    base.push([36, 52, arm, 12, 12, 8], [20, 52, 4, 12, 8, 20]); // левые рука и нога
    base.push(
      [40, 8, 8, 8, 4, 0], // шапка
      [20, 36, 8, 12, 4, 8], // куртка
      [44, 36, arm, 12, 4 - arm, 8],
      [52, 52, arm, 12, 12, 8],
      [4, 36, 4, 12, 4, 20],
      [4, 52, 4, 12, 8, 20],
    );
  } else {
    base.push([40, 8, 8, 8, 4, 0]);
  }
  return base;
}

/** Плоская миниатюра игрока спереди — для списка скинов, без WebGL. */
export function SkinThumb({ src, model, className }: { src: string | null; model: SkinModel; className?: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    if (!src) return;
    let cancelled = false;
    void loadImage(src)
      .then((img) => {
        if (cancelled || !canvas.current) return;
        const ctx = canvas.current.getContext("2d")!;
        ctx.clearRect(0, 0, 16, 32);
        ctx.imageSmoothingEnabled = false;
        const legacy = img.height === 32;
        for (const [sx, sy, w, h, dx, dy] of frontParts(model, legacy)) ctx.drawImage(img, sx, sy, w, h, dx, dy, w, h);
        // У старых скинов 64×32 левая сторона — зеркало правой
        if (legacy) {
          const arm = model === "slim" ? 3 : 4;
          ctx.save();
          ctx.scale(-1, 1);
          ctx.drawImage(img, 44, 20, arm, 12, -(12 + arm), 8, arm, 12);
          ctx.drawImage(img, 4, 20, 4, 12, -12, 20, 4, 12);
          ctx.restore();
        }
        setReady(true);
      })
      .catch(() => setReady(false));
    return () => {
      cancelled = true;
    };
  }, [src, model]);

  return (
    <canvas
      ref={canvas}
      width={16}
      height={32}
      className={cn("[image-rendering:pixelated] transition-opacity", ready ? "opacity-100" : "opacity-0", className)}
    />
  );
}

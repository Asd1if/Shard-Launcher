import { useEffect, useRef } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { cn } from "@/lib/utils";
import { buildModel, poseAt, type BbModel, type BuiltModel } from "./bbmodel";

/**
 * 3D-превью аватара Figura: все модели аватара в одной сцене, вращение мышью,
 * выбранная анимация из модели. `animation` — имя анимации или null.
 */
export function FiguraViewer({
  models,
  textures,
  animation,
  playing,
  speed,
  onAnimations,
  className,
}: {
  models: BbModel[];
  textures: Map<string, string>;
  animation: string | null;
  playing: boolean;
  speed: number;
  /** Сообщает, какие анимации есть в моделях */
  onAnimations?: (names: string[]) => void;
  className?: string;
}) {
  const box = useRef<HTMLDivElement>(null);
  const state = useRef({ animation, playing, speed, time: 0 });
  state.current.animation = animation;
  state.current.playing = playing;
  state.current.speed = speed;

  useEffect(() => {
    state.current.time = 0;
  }, [animation]);

  useEffect(() => {
    const el = box.current!;
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    renderer.setSize(el.clientWidth, el.clientHeight);
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    el.appendChild(renderer.domElement);

    const scene = new THREE.Scene();
    scene.add(new THREE.AmbientLight(0xffffff, 1.6));
    const sun = new THREE.DirectionalLight(0xffffff, 1.4);
    sun.position.set(20, 40, 30);
    scene.add(sun);

    const built: BuiltModel[] = models.map((m) => buildModel(m, textures));
    const pivot = new THREE.Group();
    built.forEach((b) => pivot.add(b.root));
    scene.add(pivot);
    onAnimations?.([...new Set(built.flatMap((b) => b.animations.map((a) => a.name)))]);

    // Кадр по размеру модели: центр по низу, камера чуть сверху
    const bounds = new THREE.Box3().setFromObject(pivot);
    const size = bounds.getSize(new THREE.Vector3());
    const center = bounds.getCenter(new THREE.Vector3());
    const radius = Math.max(size.x, size.y, size.z, 8);
    const camera = new THREE.PerspectiveCamera(40, el.clientWidth / Math.max(1, el.clientHeight), 0.1, radius * 20);
    camera.position.set(center.x - radius * 0.9, center.y + radius * 0.35, center.z + radius * 1.6);
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.target.copy(center);
    controls.enablePan = false;
    controls.enableDamping = true;
    controls.update();

    const resize = new ResizeObserver(() => {
      renderer.setSize(el.clientWidth, el.clientHeight);
      camera.aspect = el.clientWidth / Math.max(1, el.clientHeight);
      camera.updateProjectionMatrix();
    });
    resize.observe(el);

    const clock = new THREE.Clock();
    let frame = 0;
    const loop = () => {
      const delta = clock.getDelta();
      const s = state.current;
      if (s.playing) s.time += delta * s.speed;
      for (const b of built) poseAt(b, b.animations.find((a) => a.name === s.animation) ?? null, s.time);
      controls.update();
      renderer.render(scene, camera);
      frame = requestAnimationFrame(loop);
    };
    loop();

    return () => {
      cancelAnimationFrame(frame);
      resize.disconnect();
      controls.dispose();
      built.forEach((b) => b.dispose());
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, [models, textures]);

  return <div ref={box} className={cn("relative cursor-grab overflow-hidden active:cursor-grabbing [&>canvas]:absolute [&>canvas]:inset-0", className)} />;
}

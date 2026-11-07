/**
 * Модель Blockbench (`.bbmodel`) → объекты three.js и проигрыватель её анимаций.
 *
 * Поддерживается то, что нужно для превью аватара Figura: кубы с UV на гранях, иерархия групп
 * с точками вращения, встроенные и внешние текстуры, анимации костей (поворот, сдвиг, масштаб).
 * Скрипты Lua и меши произвольной формы не исполняются и не рисуются.
 */
import * as THREE from "three";

type Vec3 = [number, number, number];

interface BbFace {
  uv?: [number, number, number, number];
  texture?: number | null;
  rotation?: number;
}

interface BbElement {
  uuid: string;
  name: string;
  type?: string;
  from: Vec3;
  to: Vec3;
  origin?: Vec3;
  rotation?: Vec3;
  inflate?: number;
  visibility?: boolean;
  faces?: Partial<Record<FaceName, BbFace>>;
}

interface BbGroup {
  uuid: string;
  name: string;
  origin?: Vec3;
  rotation?: Vec3;
  visibility?: boolean;
  children: (string | BbGroup)[];
}

interface BbTexture {
  name?: string;
  path?: string;
  relative_path?: string;
  source?: string;
  uv_width?: number;
  uv_height?: number;
  width?: number;
  height?: number;
}

interface BbKeyframe {
  channel: "rotation" | "position" | "scale" | string;
  time: number;
  interpolation?: string;
  data_points: { x: string | number; y: string | number; z: string | number }[];
}

interface BbAnimation {
  name: string;
  loop?: "loop" | "once" | "hold";
  length: number;
  animators?: Record<string, { name?: string; type?: string; keyframes: BbKeyframe[] }>;
}

export interface BbModel {
  resolution?: { width: number; height: number };
  elements?: BbElement[];
  outliner?: (string | BbGroup)[];
  /** В Blockbench 5 группы лежат отдельным списком, а в outliner — только ссылки */
  groups?: BbGroup[];
  textures?: BbTexture[];
  animations?: BbAnimation[];
}

type FaceName = "north" | "south" | "east" | "west" | "up" | "down";

// Углы грани: верх-лево, верх-право, низ-лево, низ-право — как в самом Blockbench
const faceCorners: Record<FaceName, (a: Vec3, b: Vec3) => Vec3[]> = {
  north: ([x1, y1, z1], [x2, y2]) => [[x2, y2, z1], [x1, y2, z1], [x2, y1, z1], [x1, y1, z1]],
  south: ([x1, y1], [x2, y2, z2]) => [[x1, y2, z2], [x2, y2, z2], [x1, y1, z2], [x2, y1, z2]],
  east: ([, y1, z1], [x2, y2, z2]) => [[x2, y2, z2], [x2, y2, z1], [x2, y1, z2], [x2, y1, z1]],
  west: ([x1, y1, z1], [, y2, z2]) => [[x1, y2, z1], [x1, y2, z2], [x1, y1, z1], [x1, y1, z2]],
  up: ([x1, , z1], [x2, y2, z2]) => [[x1, y2, z1], [x2, y2, z1], [x1, y2, z2], [x2, y2, z2]],
  down: ([x1, y1, z1], [x2, , z2]) => [[x1, y1, z2], [x2, y1, z2], [x1, y1, z1], [x2, y1, z1]],
};

const rad = THREE.MathUtils.degToRad;

/** Поворот из Blockbench: градусы, порядок ZYX. */
function applyRotation(obj: THREE.Object3D, r: Vec3 | undefined) {
  obj.rotation.order = "ZYX";
  obj.rotation.set(rad(r?.[0] ?? 0), rad(r?.[1] ?? 0), rad(r?.[2] ?? 0));
}

function cubeGeometry(el: BbElement, origin: Vec3, uvSize: (tex: number | null | undefined) => [number, number]) {
  const inflate = el.inflate ?? 0;
  const a: Vec3 = [el.from[0] - inflate - origin[0], el.from[1] - inflate - origin[1], el.from[2] - inflate - origin[2]];
  const b: Vec3 = [el.to[0] + inflate - origin[0], el.to[1] + inflate - origin[1], el.to[2] + inflate - origin[2]];
  const positions: number[] = [];
  const uvs: number[] = [];
  const indices: number[] = [];
  const groups: { start: number; count: number; texture: number }[] = [];

  for (const name of Object.keys(faceCorners) as FaceName[]) {
    const face = el.faces?.[name];
    // Грань без текстуры в Blockbench не рисуется
    if (!face || face.texture === null || face.texture === undefined || !face.uv) continue;
    const [w, h] = uvSize(face.texture);
    const [u1, v1, u2, v2] = face.uv;
    let corners: [number, number][] = [
      [u1 / w, 1 - v1 / h],
      [u2 / w, 1 - v1 / h],
      [u1 / w, 1 - v2 / h],
      [u2 / w, 1 - v2 / h],
    ];
    // Поворот UV на 90°: TL→TR→BR→BL
    for (let r = 0; r < ((face.rotation ?? 0) / 90) % 4; r++) {
      const [tl, tr, bl, br] = corners;
      corners = [bl, tl, br, tr];
    }
    const base = positions.length / 3;
    for (const [i, p] of faceCorners[name](a, b).entries()) {
      positions.push(...p);
      uvs.push(...corners[i]);
    }
    const start = indices.length;
    indices.push(base, base + 2, base + 1, base + 2, base + 3, base + 1);
    groups.push({ start, count: 6, texture: face.texture });
  }

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return { geometry, groups };
}

/** Текстура из data URL: пиксели без сглаживания, прозрачность как в игре. */
function loadTexture(src: string) {
  const texture = new THREE.TextureLoader().load(src);
  texture.magFilter = THREE.NearestFilter;
  texture.minFilter = THREE.NearestFilter;
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

export interface BuiltModel {
  root: THREE.Group;
  animations: BbAnimation[];
  /** uuid группы → объект и его исходная поза */
  bones: Map<string, { obj: THREE.Object3D; position: THREE.Vector3; rotation: THREE.Euler }>;
  dispose: () => void;
}

/**
 * Собирает модель. `external` — текстуры из папки аватара по имени файла,
 * для моделей, где текстура не вшита.
 */
export function buildModel(model: BbModel, external: Map<string, string>): BuiltModel {
  const res = model.resolution ?? { width: 16, height: 16 };
  const textures = (model.textures ?? []).map((t) => {
    const file = (t.relative_path ?? t.path ?? t.name ?? "").split(/[\\/]/).pop() ?? "";
    const src = t.source?.startsWith("data:") ? t.source : (external.get(file) ?? external.get(`${t.name}.png`) ?? external.get(t.name ?? ""));
    return { src, w: t.uv_width ?? res.width, h: t.uv_height ?? res.height };
  });
  const materials = textures.map((t) =>
    t.src
      ? new THREE.MeshStandardMaterial({ map: loadTexture(t.src), transparent: true, alphaTest: 0.05, side: THREE.DoubleSide })
      : new THREE.MeshStandardMaterial({ color: 0x9a9aa2 }),
  );
  const fallback = new THREE.MeshStandardMaterial({ color: 0x9a9aa2 });
  const uvSize = (tex: number | null | undefined): [number, number] => {
    const t = tex === null || tex === undefined ? undefined : textures[tex];
    return [t?.w ?? res.width, t?.h ?? res.height];
  };

  const elements = new Map((model.elements ?? []).map((e) => [e.uuid, e]));
  const groupsById = new Map((model.groups ?? []).map((g) => [g.uuid, g]));
  const bones: BuiltModel["bones"] = new Map();
  const geometries: THREE.BufferGeometry[] = [];

  const addElement = (el: BbElement, parent: THREE.Object3D, parentOrigin: Vec3) => {
    if (el.visibility === false || (el.type && el.type !== "cube")) return;
    const origin = el.origin ?? [0, 0, 0];
    const { geometry, groups } = cubeGeometry(el, origin, uvSize);
    geometries.push(geometry);
    const used = [...new Set(groups.map((g) => g.texture))];
    for (const g of groups) geometry.addGroup(g.start, g.count, used.indexOf(g.texture));
    const mesh = new THREE.Mesh(geometry, used.map((i) => materials[i] ?? fallback));
    mesh.position.set(origin[0] - parentOrigin[0], origin[1] - parentOrigin[1], origin[2] - parentOrigin[2]);
    applyRotation(mesh, el.rotation);
    mesh.name = el.name;
    parent.add(mesh);
  };

  const addNode = (node: string | BbGroup, parent: THREE.Object3D, parentOrigin: Vec3) => {
    if (typeof node === "string") {
      const el = elements.get(node);
      if (el) return addElement(el, parent, parentOrigin);
      const group = groupsById.get(node);
      if (group) addNode(group, parent, parentOrigin);
      return;
    }
    const full = groupsById.get(node.uuid) ?? node;
    const children = node.children ?? full.children ?? [];
    if (full.visibility === false) return;
    const origin = full.origin ?? [0, 0, 0];
    const obj = new THREE.Group();
    obj.name = full.name;
    obj.position.set(origin[0] - parentOrigin[0], origin[1] - parentOrigin[1], origin[2] - parentOrigin[2]);
    applyRotation(obj, full.rotation);
    bones.set(full.uuid, { obj, position: obj.position.clone(), rotation: obj.rotation.clone() });
    parent.add(obj);
    for (const child of children) addNode(child, obj, origin);
  };

  const root = new THREE.Group();
  const outliner = model.outliner ?? (model.elements ?? []).map((e) => e.uuid);
  for (const node of outliner) addNode(node, root, [0, 0, 0]);

  return {
    root,
    animations: model.animations ?? [],
    bones,
    dispose: () => {
      geometries.forEach((g) => g.dispose());
      materials.forEach((m) => {
        m.map?.dispose();
        m.dispose();
      });
      fallback.dispose();
    },
  };
}

/** Число из ключевого кадра. Выражения Molang не считаем — кроме простых чисел берём 0. */
function num(v: string | number | undefined) {
  if (typeof v === "number") return v;
  const n = parseFloat(String(v ?? "0").trim());
  return Number.isFinite(n) ? n : 0;
}

function sample(frames: BbKeyframe[], time: number): Vec3 | null {
  if (frames.length === 0) return null;
  const sorted = frames.toSorted((a, b) => a.time - b.time);
  const value = (k: BbKeyframe): Vec3 => {
    const p = k.data_points[0] ?? { x: 0, y: 0, z: 0 };
    return [num(p.x), num(p.y), num(p.z)];
  };
  if (time <= sorted[0].time) return value(sorted[0]);
  const last = sorted[sorted.length - 1];
  if (time >= last.time) return value(last);
  const i = sorted.findIndex((k) => k.time > time);
  const [a, b] = [sorted[i - 1], sorted[i]];
  if (a.interpolation === "step") return value(a);
  const t = (time - a.time) / (b.time - a.time || 1);
  const [va, vb] = [value(a), value(b)];
  return [va[0] + (vb[0] - va[0]) * t, va[1] + (vb[1] - va[1]) * t, va[2] + (vb[2] - va[2]) * t];
}

/**
 * Ставит позу анимации на момент `time` (секунды). Знаки — как в превью Blockbench:
 * поворот по X и Y и сдвиг по X инвертированы.
 */
export function poseAt(model: BuiltModel, animation: BbAnimation | null, time: number) {
  for (const { obj, position, rotation } of model.bones.values()) {
    obj.position.copy(position);
    obj.rotation.copy(rotation);
    obj.scale.set(1, 1, 1);
  }
  if (!animation) return;
  const length = animation.length || 1;
  const t = animation.loop === "loop" ? time % length : Math.min(time, length);
  for (const [uuid, animator] of Object.entries(animation.animators ?? {})) {
    const bone = model.bones.get(uuid) ?? [...model.bones.values()].find((b) => b.obj.name === animator.name);
    if (!bone) continue;
    const byChannel = (c: string) => animator.keyframes.filter((k) => k.channel === c);
    const r = sample(byChannel("rotation"), t);
    if (r) {
      bone.obj.rotation.x -= rad(r[0]);
      bone.obj.rotation.y -= rad(r[1]);
      bone.obj.rotation.z += rad(r[2]);
    }
    const p = sample(byChannel("position"), t);
    if (p) bone.obj.position.add(new THREE.Vector3(-p[0], p[1], p[2]));
    const s = sample(byChannel("scale"), t);
    if (s) bone.obj.scale.set(s[0] || 1, s[1] || 1, s[2] || 1);
  }
}

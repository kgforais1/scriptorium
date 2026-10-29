// Scene file (skins/<id>/scene.json) types and loader. A missing or malformed key is a load
// error, never a silent fallback (design/DESIGN.md, skin model rules).

export type Rgb = [number, number, number];

export type PointLight = { type: "point"; position: Rgb; color: Rgb; poolCenter: [number, number] };
export type DirectionalLight = { type: "directional"; direction: Rgb; color: Rgb };

export type Scene = {
  skin: string;
  camera: { fovDegrees: number };
  background: { srgb: Rgb };
  light: PointLight | DirectionalLight;
  ambient: Rgb;
  exposure: number;
  vignette: number;
  material: {
    coverLeather: { dark: Rgb; light: Rgb };
    gilt: Rgb;
    ribbon: Rgb;
    pageEdge: Rgb;
    paper: Rgb;
    desk: { light: Rgb; mid: Rgb; dark: Rgb };
  };
};

export class SceneError extends Error {}

type Obj = Record<string, unknown>;

function obj(v: unknown, path: string): Obj {
  if (typeof v !== "object" || v === null || Array.isArray(v)) throw new SceneError(`${path}: expected an object`);
  return v as Obj;
}

function num(v: unknown, path: string): number {
  if (typeof v !== "number" || !Number.isFinite(v)) throw new SceneError(`${path}: expected a number`);
  return v;
}

function vec<N extends 2 | 3>(v: unknown, n: N, path: string): N extends 2 ? [number, number] : Rgb {
  if (!Array.isArray(v) || v.length !== n) throw new SceneError(`${path}: expected ${n} numbers`);
  return v.map((x, i) => num(x, `${path}[${i}]`)) as N extends 2 ? [number, number] : Rgb;
}

function unit(v: unknown, path: string): number {
  const n = num(v, path);
  if (n < 0 || n > 1) throw new SceneError(`${path}: must be between 0 and 1`);
  return n;
}

function str(v: unknown, path: string): string {
  if (typeof v !== "string" || !v) throw new SceneError(`${path}: expected a string`);
  return v;
}

function parseLight(raw: unknown): Scene["light"] {
  const l = obj(raw, "light");
  const color = vec(l.color, 3, "light.color");
  if (l.type === "point") {
    return {
      type: "point",
      position: vec(l.position, 3, "light.position"),
      color,
      poolCenter: vec(l.poolCenter, 2, "light.poolCenter"),
    };
  }
  if (l.type === "directional") {
    const d = vec(l.direction, 3, "light.direction");
    if (Math.hypot(...d) < 1e-6) throw new SceneError("light.direction: must not be zero");
    return { type: "directional", direction: d, color };
  }
  throw new SceneError(`light.type: expected "point" or "directional", got ${JSON.stringify(l.type)}`);
}

export function parseScene(raw: unknown): Scene {
  const s = obj(raw, "scene");
  const cam = obj(s.camera, "camera");
  const bg = obj(s.background, "background");
  const m = obj(s.material, "material");
  const leather = obj(m.coverLeather, "material.coverLeather");
  const desk = obj(m.desk, "material.desk");
  const fov = num(cam.fovDegrees, "camera.fovDegrees");
  if (fov <= 1 || fov >= 120) throw new SceneError("camera.fovDegrees: must be between 1 and 120");
  return {
    skin: str(s.skin, "skin"),
    camera: { fovDegrees: fov },
    background: { srgb: vec(bg.srgb, 3, "background.srgb") },
    light: parseLight(s.light),
    ambient: vec(s.ambient, 3, "ambient"),
    exposure: num(s.exposure, "exposure"),
    vignette: unit(s.vignette, "vignette"),
    material: {
      coverLeather: {
        dark: vec(leather.dark, 3, "material.coverLeather.dark"),
        light: vec(leather.light, 3, "material.coverLeather.light"),
      },
      gilt: vec(m.gilt, 3, "material.gilt"),
      ribbon: vec(m.ribbon, 3, "material.ribbon"),
      pageEdge: vec(m.pageEdge, 3, "material.pageEdge"),
      paper: vec(m.paper, 3, "material.paper"),
      desk: {
        light: vec(desk.light, 3, "material.desk.light"),
        mid: vec(desk.mid, 3, "material.desk.mid"),
        dark: vec(desk.dark, 3, "material.desk.dark"),
      },
    },
  };
}

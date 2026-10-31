import { describe, expect, it } from "vitest";
import daylight from "../../../../design/skins/daylight/scene.json";
import lamplit from "../../../../design/skins/lamplit/scene.json";
import { parseScene, SceneError } from "./scene";

type Json = { [key: string]: unknown };

// Copies a scene file and sets (or, with undefined, removes) the value at a dotted path.
function edit(base: unknown, path: string, value: unknown): Json {
  const root = structuredClone(base) as Json;
  const keys = path.split(".");
  const last = keys.pop()!;
  const parent = keys.reduce<Json>((o, k) => o[k] as Json, root);
  if (value === undefined) delete parent[last];
  else parent[last] = value;
  return root;
}

describe("parseScene", () => {
  it("parses the lamplit skin (point light)", () => {
    const s = parseScene(lamplit);
    expect(s.skin).toBe("lamplit");
    expect(s.light.type).toBe("point");
    expect(s.material.paper).toHaveLength(3);
    expect(s.material.desk.mid).toHaveLength(3);
  });

  it("parses the daylight scene (directional light)", () => {
    const s = parseScene(daylight);
    expect(s.light.type).toBe("directional");
  });

  it.each([null, 5, "x", [], undefined])("rejects a non-object root: %s", (raw) => {
    expect(() => parseScene(raw)).toThrow(SceneError);
  });

  it("rejects a missing key and names its path", () => {
    expect(() => parseScene(edit(lamplit, "material.paper", undefined))).toThrow(/material\.paper/);
  });

  it("rejects a missing section", () => {
    expect(() => parseScene(edit(lamplit, "material", undefined))).toThrow(/material: expected an object/);
  });

  it("rejects a vector of the wrong length", () => {
    expect(() => parseScene(edit(lamplit, "ambient", [1, 1]))).toThrow(/ambient: expected 3 numbers/);
  });

  it("rejects a non-number inside a vector", () => {
    expect(() => parseScene(edit(lamplit, "background.srgb", [0, "x", 0]))).toThrow(/background\.srgb\[1\]/);
  });

  it("rejects NaN and infinite numbers", () => {
    expect(() => parseScene(edit(lamplit, "exposure", Number.NaN))).toThrow(/exposure/);
    expect(() => parseScene(edit(lamplit, "exposure", Infinity))).toThrow(/exposure/);
  });

  it.each([
    ["camera.fovDegrees", 0, /fovDegrees/],
    ["camera.fovDegrees", 121, /fovDegrees/],
    ["vignette", 1.5, /vignette/],
    ["vignette", -0.1, /vignette/],
    ["exposure", -1, /exposure/],
  ] as const)("rejects %s = %s", (path, value, re) => {
    expect(() => parseScene(edit(lamplit, path, value))).toThrow(re);
  });

  it("rejects an empty skin id", () => {
    expect(() => parseScene(edit(lamplit, "skin", ""))).toThrow(/skin/);
  });

  it("rejects an unknown light type", () => {
    expect(() => parseScene(edit(lamplit, "light.type", "spot"))).toThrow(/light\.type/);
  });

  it("rejects a point light without a position", () => {
    expect(() => parseScene(edit(lamplit, "light.position", undefined))).toThrow(/light\.position/);
  });

  it("rejects a zero directional light", () => {
    expect(() => parseScene(edit(daylight, "light.direction", [0, 0, 0]))).toThrow(/must not be zero/);
  });
});

import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.{ts,tsx}"],
    coverage: {
      provider: "v8",
      include: ["src/lib/**", "src/features/reader/engine/**"],
      exclude: [
        "src/**/*.test.{ts,tsx}",
        // GL, shader, and pointer-event code needs a GPU and a real canvas, so it has no unit
        // tests. The logic they drive is split into the pure modules above, which do.
        "src/features/reader/engine/{engine,renderer,glBackend,gl,shaders,pointerInput,index}.ts",
      ],
      reporter: ["text", "lcov"],
      thresholds: { lines: 80 },
    },
  },
});

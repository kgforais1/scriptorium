import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.{ts,tsx}"],
    coverage: {
      provider: "v8",
      include: ["src/lib/**", "src/features/reader/engine/**", "src/features/reader/keys.ts", "src/features/reader/labels.ts"],
      exclude: [
        "src/**/*.test.{ts,tsx}",
        // GL, shader, and DOM-event glue needs a GPU or a real canvas, so it has no unit tests.
        // The decisions it acts on are in the pure modules that are covered: turnMachine,
        // pageTextures, gesture (tap versus drag, settle targets), camera, corners.
        // What stays untested is the wiring itself: draw order, listener setup, and teardown.
        "src/features/reader/engine/{engine,renderer,gl,shaders,pointerInput,decodeWorker,workerFactory,index}.ts",
      ],
      reporter: ["text", "lcov"],
      thresholds: { lines: 80 },
    },
  },
});

import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.{ts,tsx}"],
    // Do not fail while a glob has no tests yet.
    passWithNoTests: true,
    coverage: {
      provider: "v8",
      include: ["src/lib/**"],
      exclude: ["src/lib/**/*.test.{ts,tsx}"],
      reporter: ["text", "lcov"],
      // Report even files that no test imports, so an untested module cannot hide.
      all: true,
      thresholds: { lines: 80 },
    },
  },
});

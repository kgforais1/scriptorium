// ESLint flat config for the Vite + React + TypeScript frontend.
// Gates: complexity <= 15, doc comments on exported functions and classes, React hooks rules.
import js from "@eslint/js";
import globals from "globals";
import jsdoc from "eslint-plugin-jsdoc";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      "dist/**",
      "coverage/**",
      "node_modules/**",
      ".venv/**",
      "src-tauri/**",
      "tmp/**",
      "**/*.d.ts",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  reactHooks.configs.flat.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      globals: { ...globals.browser },
    },
    plugins: { jsdoc },
    rules: {
      complexity: ["error", 15],
      // Exported functions and classes only. Local helpers need no doc comment.
      "jsdoc/require-jsdoc": [
        "error",
        {
          publicOnly: true,
          require: {
            FunctionDeclaration: true,
            ClassDeclaration: true,
            ArrowFunctionExpression: true,
            FunctionExpression: true,
            MethodDefinition: false,
          },
          contexts: [],
        },
      ],
    },
  },
  {
    // Config files run in Node.
    files: ["*.config.{js,ts}"],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    // Throwaway flipbook spike (ADR-0001). It is rewritten in Phase 3, so it is exempt from
    // the complexity limit, the doc-comment rule, and the two React Compiler rules it trips.
    // Do not add new files here.
    files: ["src/spike/flipbook/**"],
    rules: {
      complexity: "off",
      "jsdoc/require-jsdoc": "off",
      "react-hooks/refs": "off",
      "react-hooks/set-state-in-effect": "off",
    },
  },
);

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
    // Build and check scripts run in Node. Same complexity limit as the app code.
    files: ["scripts/**/*.mjs"],
    languageOptions: { globals: { ...globals.node } },
    rules: { complexity: ["error", 15] },
  },
  {
    // Config files run in Node.
    files: ["*.config.{js,ts}"],
    languageOptions: { globals: { ...globals.node } },
  },
);

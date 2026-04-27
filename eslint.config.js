// @ts-check
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import svelte from "eslint-plugin-svelte";
import globals from "globals";

export default tseslint.config(
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs["flat/recommended"],
  ...svelte.configs["flat/prettier"],
  {
    languageOptions: {
      globals: { ...globals.browser, ...globals.es2022, ...globals.node },
    },
  },
  {
    files: ["**/*.svelte"],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
    },
    rules: {
      // Svelte 5 runes are globals
      "no-undef": "off",
    },
  },
  {
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      "@typescript-eslint/consistent-type-imports": [
        "error",
        { prefer: "type-imports", fixStyle: "inline-type-imports" },
      ],
      // We rely on Result<T, E> patterns; no-throw-literal stays warn.
      "no-throw-literal": "warn",
    },
  },
  {
    ignores: [
      "node_modules/**",
      "dist/**",
      "build/**",
      "src-tauri/**",
      "_spike/**",
      "src/lib/ipc/**", // generated specta bindings
      // Svelte 5 .svelte.ts (runes-bearing TS) needs svelte-eslint-parser
      // configured for the double-extension. Until that's wired, type
      // safety is covered by svelte-check.
      "**/*.svelte.ts",
      "**/*.svelte.js",
    ],
  },
);

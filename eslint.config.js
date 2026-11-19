import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import pluginVue from "eslint-plugin-vue";

export default tseslint.config(
  {
    ignores: ["dist/**", "node_modules/**", "src-tauri/gen/**", "src-tauri/target/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs["flat/recommended"],
  {
    files: ["src/**/*.vue"],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
    },
  },
  {
    files: ["src/**/*.{ts,tsx,vue}", "scripts/**/*.mjs", "vite.config.ts"],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
    rules: {
      "vue/max-attributes-per-line": "off",
      "vue/html-indent": "off",
      "vue/html-self-closing": "off",
      "vue/multi-word-component-names": "off",
      "vue/singleline-html-element-content-newline": "off",
    },
  },
);

import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    languageOptions: {
      globals: { document: "readonly", fetch: "readonly", console: "readonly" },
    },
  },
  { ignores: ["dist/**"] },
);

import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  js.configs.recommended,
  // Type-aware linting. The plain recommended set only catches `any` that is
  // written down; these rules also catch `any` that leaks in from untyped
  // boundaries such as JSON.parse or a library without types.
  ...tseslint.configs.recommendedTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
      globals: { document: "readonly", fetch: "readonly", console: "readonly" },
    },
    rules: {
      // TypeScript is only worth having if `any` cannot silently spread.
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-unsafe-assignment": "error",
      "@typescript-eslint/no-unsafe-member-access": "error",
      "@typescript-eslint/no-unsafe-call": "error",
      "@typescript-eslint/no-unsafe-return": "error",
      "@typescript-eslint/no-unsafe-argument": "error",
    },
  },
  { ignores: ["dist/**", "eslint.config.js", "vite.config.ts"] },
);

import js from "@eslint/js";
import tseslint from "typescript-eslint";

/**
 * The same rule set as apps/web/eslint.config.js, applied to the end-to-end
 * layer. `tsc` alone accepts an `any` that is written down, and nothing else
 * reads these files before they run, so without this the specs — including the
 * S11 guard in tests/without-pontis.spec.ts — are where `any` could spread
 * unseen. Keep the two rule blocks in step.
 */
export default tseslint.config(
  js.configs.recommended,
  ...tseslint.configs.recommendedTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
      // scripts/serve.js runs under Node; the specs import what they use.
      globals: { process: "readonly", console: "readonly", Buffer: "readonly" },
    },
    rules: {
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-unsafe-assignment": "error",
      "@typescript-eslint/no-unsafe-member-access": "error",
      "@typescript-eslint/no-unsafe-call": "error",
      "@typescript-eslint/no-unsafe-return": "error",
      "@typescript-eslint/no-unsafe-argument": "error",
    },
  },
  { ignores: ["eslint.config.js", "test-results/**", "playwright-report/**"] },
);

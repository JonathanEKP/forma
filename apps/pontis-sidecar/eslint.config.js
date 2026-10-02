import js from "@eslint/js";
import tseslint from "typescript-eslint";

// The same rules as apps/web: `any` may neither be written nor leak in from an
// untyped boundary, and every warning fails the run.
export default tseslint.config(
  js.configs.recommended,
  ...tseslint.configs.recommendedTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // TypeScript already resolves every name, Node globals included;
      // typescript-eslint recommends leaving this rule to the compiler.
      "no-undef": "off",
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-unsafe-assignment": "error",
      "@typescript-eslint/no-unsafe-member-access": "error",
      "@typescript-eslint/no-unsafe-call": "error",
      "@typescript-eslint/no-unsafe-return": "error",
      "@typescript-eslint/no-unsafe-argument": "error",
      // node:test's describe/it return promises the runner itself awaits.
      "@typescript-eslint/no-floating-promises": [
        "error",
        { allowForKnownSafeCalls: [{ from: "package", package: "node:test", name: ["describe", "it"] }] },
      ],
    },
  },
  { ignores: ["eslint.config.js", "test-driver/**", ".pontis/**"] },
);

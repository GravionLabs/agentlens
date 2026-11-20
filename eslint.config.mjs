import tseslint from "typescript-eslint";
import angular from "angular-eslint";

/**
 * ESLint lints the Angular templates only, because oxlint cannot parse them (ADR 0006 of
 * GravionLabs/skills, #542). TypeScript, including the Angular class rules, is linted by oxlint
 * (`.oxlintrc.json`). Every component template is a `.html` file, so no inline-template processor
 * is needed.
 */
export default tseslint.config(
  {
    ignores: [
      "dist/**",
      ".angular/**",
      "node_modules/**",
      ".agents/**",
      ".claude/**",
      "src-tauri/**",
      "crates/**",
      "target/**",
      "site/.vitepress/dist/**",
      "site/.vitepress/cache/**",
      // The lint probe holds deliberate violations; src/testing/lint-probe.spec.ts lints it.
      "src/testing/lint-probe/**",
    ],
  },
  {
    files: ["**/*.html"],
    extends: [...angular.configs.templateRecommended, ...angular.configs.templateAccessibility],
  },
);

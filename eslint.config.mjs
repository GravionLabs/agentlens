import tseslint from "typescript-eslint";
import angular from "angular-eslint";
import { readFileSync } from "node:fs";
import helix, { helixAllow } from "@gravionlabs/helix-lint/eslint";

// Exceptions to the helix-lint preset, each with a reason (#559): scripts/lint/helix-allow.json.
const allow = JSON.parse(
  readFileSync(new URL("./scripts/lint/helix-allow.json", import.meta.url), "utf8"),
);

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
  // helix/no-raw-control (#559): no native button, input, select, textarea or progress in a
  // template. Templates are `.html` files, so no inline-template processor is needed.
  ...helix,
  ...allow.eslint.map((entry) => helixAllow(entry)),
);

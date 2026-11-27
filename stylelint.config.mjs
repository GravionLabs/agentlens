import { readFileSync } from "node:fs";
import { helixAllow } from "@gravionlabs/helix-lint/stylelint";

/**
 * Stylelint runs the helix-lint preset over the stylesheets and the inline `styles` of the
 * components (#559): no hex, rgb or hsl colors, font sizes only from the type scale. Exceptions go
 * in `scripts/lint/helix-allow.json`, each with a reason.
 */
const allow = JSON.parse(
  readFileSync(new URL("./scripts/lint/helix-allow.json", import.meta.url), "utf8"),
);

export default {
  extends: ["@gravionlabs/helix-lint/stylelint"],
  // Next to the preset (#559): component CSS styles no native control (the Helix directive owns
  // its look), and the `font` shorthand must not carry a px or rem size (the preset checks only
  // `font-size`).
  rules: {
    // A type selector only (not `.button`, `#input`, `hx-select` or an attribute value), and no
    // `.btn*` class: the hand-built button system is gone (#557).
    "selector-disallowed-list": [
      /(^|[\s>+~(,])(button|input|select|textarea|progress|\.btn[\w-]*)(?![\w-])/i,
    ],
    "declaration-property-value-disallowed-list": { font: [/\d(px|rem)\b/i] },
  },
  ignoreFiles: [
    "dist/**",
    "node_modules/**",
    ".angular/**",
    "site/**",
    "src-tauri/**",
    "crates/**",
    "target/**",
    "**/*.spec.ts",
  ],
  overrides: [
    // The components keep their styles inline in the `.ts` files.
    { files: ["src/**/*.ts"], customSyntax: "postcss-angular" },
    ...allow.stylelint.map((entry) => helixAllow(entry)),
  ],
};

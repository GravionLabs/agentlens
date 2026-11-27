import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { ESLint } from "eslint";
import { helixAllow as eslintAllow } from "@gravionlabs/helix-lint/eslint";
import { helixAllow as stylelintAllow } from "@gravionlabs/helix-lint/stylelint";
import stylelint from "stylelint";
import { describe, expect, it } from "vitest";

/**
 * #542: oxlint runs the Angular class rules through an alpha JS plugin and the promise rules with
 * type information, ESLint the template rules. `lint-probe/` holds deliberate violations (left out
 * of `pnpm lint`); this spec lints it and expects every rule id, so a JS plugin, type-aware or
 * config regression fails here instead of linting nothing.
 */
const PROBE = "src/testing/lint-probe";
// Both linters start a process or load their plugins; under load that exceeds the default 5 s.
const TIMEOUT = 60_000;

interface OxlintReport {
  diagnostics: { code: string; filename: string; severity: string }[];
}

function oxlint(): OxlintReport["diagnostics"] {
  let out: string;
  try {
    out = execFileSync("node_modules/.bin/oxlint", ["-f", "json", PROBE], { encoding: "utf8" });
  } catch (error) {
    // oxlint exits 1 when it reports errors; the report is still on stdout.
    out = (error as { stdout: string }).stdout;
  }
  return (JSON.parse(out) as OxlintReport).diagnostics;
}

describe("lint probe", () => {
  it(
    "oxlint reports the Angular class violations, the floating promise and the inline template",
    () => {
      const codes = oxlint().map((d) => `${d.filename.replace(`${PROBE}/`, "")} ${d.code}`);
      expect(codes.sort()).toEqual([
        "inline-template.component.ts @angular-eslint(component-max-inline-declarations)",
        "probe.component.ts @angular-eslint(component-selector)",
        "probe.component.ts @angular-eslint(no-empty-lifecycle-method)",
        "probe.component.ts @angular-eslint(no-output-on-prefix)",
        "probe.component.ts @angular-eslint(prefer-inject)",
        "probe.component.ts @angular-eslint(use-lifecycle-interface)",
        "probe.component.ts typescript(no-floating-promises)",
      ]);
    },
    TIMEOUT,
  );

  it(
    "ESLint reports the template violations",
    async () => {
      const [result] = await new ESLint({ ignore: false }).lintFiles([
        `${PROBE}/probe.component.html`,
      ]);
      expect(result.messages.map((m) => m.ruleId ?? "").sort()).toEqual([
        "@angular-eslint/template/alt-text",
        "@angular-eslint/template/click-events-have-key-events",
        "@angular-eslint/template/interactive-supports-focus",
        // The raw <button> of the probe (#559).
        "helix/no-raw-control",
      ]);
    },
    TIMEOUT,
  );

  it(
    "Stylelint reports the hex color, the off-scale font size, the styled native control and the .btn class in the stylesheet and the inline styles (#559)",
    async () => {
      const { results } = await stylelint.lint({ files: `${PROBE}/*.{css,ts}`, ignorePath: [] });
      const reported = results.flatMap((r) =>
        r.warnings.map((w) => `${r.source?.replace(/^.*\//, "")} ${w.rule}`),
      );
      expect(reported.sort()).toEqual([
        "probe.component.css color-no-hex",
        "probe.component.css declaration-property-value-allowed-list",
        "probe.component.css declaration-property-value-disallowed-list",
        "probe.component.css selector-disallowed-list",
        "styles-probe.component.ts color-no-hex",
        "styles-probe.component.ts declaration-property-value-allowed-list",
        "styles-probe.component.ts declaration-property-value-disallowed-list",
        "styles-probe.component.ts selector-disallowed-list",
        "styles-probe.component.ts selector-disallowed-list",
        "styles-probe.component.ts selector-disallowed-list",
      ]);
    },
    TIMEOUT,
  );

  it("every allowlist entry names a reason; an entry without one throws (#559)", () => {
    const allow = JSON.parse(readFileSync("scripts/lint/helix-allow.json", "utf8")) as {
      eslint: { files: string[]; rules: string[]; reason: string }[];
      stylelint: { files: string[]; rules: string[]; reason: string }[];
    };
    for (const entry of [...allow.eslint, ...allow.stylelint]) {
      expect(entry.reason.trim().length).toBeGreaterThan(10);
      // One entry per file: a glob would hide new violations.
      for (const file of entry.files) expect(file).not.toMatch(/\*/);
    }
    const bare = { files: ["src/**"], rules: ["helix/no-raw-control"] };
    expect(() => eslintAllow(bare)).toThrow(/reason/);
    expect(() => stylelintAllow(bare)).toThrow(/reason/);
  });
});

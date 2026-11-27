import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(path, "utf8");
const theme = read("src/helix-theme.css");
const tokens = read("node_modules/@gravionlabs/helix-ui/tokens.css");
const shell = read("node_modules/@gravionlabs/helix-shell/styles.css");
const styles = read("src/styles.css");
const chart = read("src/styles/chart-colors.css");

/** The global style sheets and the TypeScript of the app (the component styles are inline). */
const sources = [
  "src/styles.css",
  "src/helix-theme.css",
  ...readdirSync("src/app", { recursive: true, encoding: "utf8" })
    .filter((file) => file.endsWith(".ts") && !file.endsWith(".spec.ts"))
    .map((file) => `src/app/${file}`),
];

const withoutComments = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, "");
const declared = (css: string) => new Set([...css.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]));
const used = (css: string) => [...css.matchAll(/var\((--[\w-]+)/g)].map((m) => m[1]);
/** The sources with a match of `pattern`, with the matches. */
const findings = (pattern: RegExp) =>
  sources.flatMap((file) =>
    [...withoutComments(read(file)).matchAll(pattern)].map((m) => `${file}: ${m[0]}`),
  );

describe("helix theme", () => {
  it("overrides only tokens that helix defines (a typo would silently do nothing)", () => {
    const known = new Set([...declared(tokens), ...declared(shell)]);
    expect([...declared(theme)].filter((name) => !known.has(name))).toEqual([]);
  });

  it("keeps the helix palette: it overrides shape and type only", () => {
    expect([...declared(theme)].sort()).toEqual([
      "--h-border-radius-md",
      "--h-form-field-shadow",
      "--helix-font-sans",
    ]);
  });

  it("reads only tokens that helix or the chart series define", () => {
    const known = new Set([...declared(tokens), ...declared(shell), ...declared(chart)]);
    const unknown = sources.flatMap((file) =>
      used(read(file))
        .filter((name) => !known.has(name))
        .map((name) => `${file}: ${name}`),
    );
    expect(unknown).toEqual([]);
  });

  it("defines every chart series for light and dark", () => {
    const [light, dark] = withoutComments(chart).split(":root.app-dark");
    expect(declared(light).size).toBe(4);
    expect(declared(dark)).toEqual(declared(light));
  });

  it("has no literal colors outside the chart series", () => {
    expect(findings(/#(?:[\da-f]{8}|[\da-f]{6}|[\da-f]{3,4})\b|\b(?:rgb|hsl)a?\(/gi)).toEqual([]);
  });

  it("takes every font size from the type scale", () => {
    expect(findings(/font-size:(?!\s*var\(--h-font-size-)[^;]*;/g)).toEqual([]);
    expect(findings(/(?<![\w-])font:[^;]*\d(?:px|rem)[^;]*;/g)).toEqual([]);
  });

  it("leaves the look of the controls to helix", () => {
    const selectors = [...withoutComments(styles).matchAll(/[^{}]+(?=\{)/g)].map((m) => m[0]);
    const controls = /(?<![\w.#-])(?:button|input|select)(?![\w-])|\.btn(?![a-z])/;
    expect(selectors.filter((selector) => controls.test(selector))).toEqual([]);
  });
});

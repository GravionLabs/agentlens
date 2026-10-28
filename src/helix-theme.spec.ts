import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(path, "utf8");
const theme = read("src/helix-theme.scss");
const tokens = read("node_modules/@gravionlabs/helix-ui/tokens.css");
const shell = read("node_modules/@gravionlabs/helix-shell/styles.css");
const styles = read("src/styles.scss");

const declared = (css: string) => new Set([...css.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]));
const used = (css: string) => [...css.matchAll(/var\((--[\w-]+)/g)].map((m) => m[1]);

describe("helix theme", () => {
  const overrides = [...declared(theme)];

  it("overrides only tokens that helix defines (a typo would silently do nothing)", () => {
    const known = new Set([...declared(tokens), ...declared(shell)]);
    expect(overrides.filter((name) => !known.has(name))).toEqual([]);
  });

  it("reads only AgentLens tokens or tokens it sets itself", () => {
    const known = new Set([...declared(styles), ...overrides]);
    expect(used(theme).filter((name) => !known.has(name))).toEqual([]);
  });

  it("binds the status bar chrome to the app panel, so there is no seam", () => {
    expect(theme).toMatch(/--helix-surface-chrome:\s*var\(--panel\)/);
  });
});

import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { assemble, PIECES } from "../scripts/assemble.mts";

const dirs: string[] = [];

afterEach(() => {
  for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true });
});

function repo(pieces: Array<keyof typeof PIECES>): string {
  const dir = mkdtempSync(join(tmpdir(), "assemble-"));
  dirs.push(dir);
  for (const piece of pieces) {
    const folder = join(dir, PIECES[piece]);
    mkdirSync(folder, { recursive: true });
    writeFileSync(join(folder, "index.html"), `<p>${piece}</p>`);
  }
  return dir;
}

describe("assemble", () => {
  it("puts the guide at the root and the application under /app/", () => {
    const dir = repo(["site", "app"]);
    const out = join(dir, "pages");
    assemble(dir, out);
    expect(readFileSync(join(out, "index.html"), "utf8")).toBe("<p>site</p>");
    expect(readFileSync(join(out, "app/index.html"), "utf8")).toBe("<p>app</p>");
    expect(existsSync(join(out, ".nojekyll"))).toBe(true);
  });

  it("empties the output folder first", () => {
    const dir = repo(["site", "app"]);
    const out = join(dir, "pages");
    mkdirSync(out);
    writeFileSync(join(out, "old.html"), "x");
    assemble(dir, out);
    expect(existsSync(join(out, "old.html"))).toBe(false);
  });

  it("refuses to publish a half-built site and names the missing piece", () => {
    const out = join(repo([]), "pages");
    expect(() => assemble(repo(["site"]), out)).toThrow(/app is not built/);
    expect(() => assemble(repo(["app"]), out)).toThrow(/site is not built/);
  });
});

// Puts the pieces of the public site into one folder, the artifact that GitHub Pages serves:
//
//   /       the documentation site (VitePress, `site/`)
//   /app/   the application with its sample data, as a live demo (`pnpm build:demo`)
//
// Usage: node site/scripts/assemble.mts <out>   (from the repository root, after both builds; see pages.yml)
import { cpSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/** Where each piece is built, relative to the top folder of the repository. */
export const PIECES = {
  site: "site/.vitepress/dist",
  app: "dist/agentlens/browser",
} as const;

/**
 * Copies the built pieces from `repo` into `out` (which is emptied first). Throws, naming the piece,
 * when one is missing: a half-built site must not be published.
 */
export function assemble(repo: string, out: string): void {
  for (const [piece, path] of Object.entries(PIECES)) {
    if (!existsSync(join(repo, path, "index.html"))) {
      throw new Error(`The ${piece} is not built: ${path}/index.html is missing.`);
    }
  }
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  cpSync(join(repo, PIECES.site), out, { recursive: true });
  cpSync(join(repo, PIECES.app), join(out, "app"), { recursive: true });
  // The files are served as they are; Jekyll would hide folders that start with an underscore.
  writeFileSync(join(out, ".nojekyll"), "");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const out = process.argv[2];
  if (!out) {
    console.error("Usage: node site/scripts/assemble.mts <out>");
    process.exit(2);
  }
  const repo = resolve(fileURLToPath(new URL("../..", import.meta.url)));
  assemble(repo, resolve(out));
  console.log(`The site is in ${resolve(out)}`);
}

import { copyFileSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitepress";
import { repoFilesOnDisk, repoLinksPlugin, REPO_URL } from "./repo-links";
import { sidebarOf } from "./sidebar";

const repoRoot = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const files = repoFilesOnDisk(repoRoot);
// `/<repository>/` on GitHub Pages; the workflow sets SITE_BASE.
const base = process.env["SITE_BASE"] ?? "/";
const readme = readFileSync(resolve(repoRoot, "docs/guide/README.md"), "utf8");
// The icon of the application is the logo of the site; it is served from where it is.
const logo = resolve(repoRoot, "src/favicon.svg");

export default defineConfig({
  title: "AgentLens",
  description:
    "See what your coding agents cost in tokens: skills, context, tools and instruction files, measured where the client reports it and estimated where it does not.",
  lang: "en",
  base,
  // The site is the documentation of the repository: its pages are the files of `docs`.
  srcDir: "../docs",
  // Decisions, and anything not listed as a page, stay on GitHub.
  srcExclude: ["adr/**"],
  // `docs/guide/README.md` is what GitHub shows for the folder, and the home of the guide here.
  rewrites: { "guide/README.md": "guide/index.md" },
  cleanUrls: true,
  lastUpdated: false,
  // A link that leads nowhere fails the build. Only the example addresses of a local run are exempt.
  ignoreDeadLinks: [/^https?:\/\/localhost(:\d+)?(\/|$)/],
  head: [["link", { rel: "icon", type: "image/svg+xml", href: `${base}logo.svg` }]],
  markdown: {
    config: (md) => repoLinksPlugin(md as never, files),
  },
  vite: {
    plugins: [
      {
        name: "agentlens-logo",
        configureServer(server) {
          server.middlewares.use(`${base}logo.svg`, (_req, res) => {
            res.setHeader("content-type", "image/svg+xml");
            res.end(readFileSync(logo));
          });
        },
      },
    ],
  },
  buildEnd: (site) => copyFileSync(logo, join(site.outDir, "logo.svg")),
  themeConfig: {
    logo: "/logo.svg",
    siteTitle: "AgentLens",
    nav: [
      { text: "Guide", link: "/guide/" },
      // Not a page of this site (the workflow adds it next to it): a full page load.
      { text: "Try the app", link: "/app/", target: "_self" },
      { text: "GitHub", link: REPO_URL },
    ],
    sidebar: sidebarOf(readme),
    search: { provider: "local" },
    editLink: {
      pattern: `${REPO_URL}/edit/main/docs/:path`,
      text: "Edit this page on GitHub",
    },
    socialLinks: [{ icon: "github", link: REPO_URL }],
    footer: { message: "Released under the MIT license." },
  },
});

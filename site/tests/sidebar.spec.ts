import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { guidePages, sidebarOf } from "../.vitepress/sidebar";

const guide = resolve(__dirname, "../../docs/guide");
const readme = readFileSync(resolve(guide, "README.md"), "utf8");

describe("guidePages", () => {
  it("reads the numbered list in its order and resolves links next to the guide", () => {
    const text = [
      "1. [Getting started](getting-started.md): first steps",
      "2. [Telemetry](../telemetry.md#sources): imports",
      "- [not numbered](nope.md)",
    ].join("\n");
    expect(guidePages(text)).toEqual([
      { text: "Getting started", link: "/guide/getting-started" },
      { text: "Telemetry", link: "/telemetry" },
    ]);
  });

  it("puts the overview first", () => {
    expect(sidebarOf("1. [A](a.md)")[0].items[0]).toEqual({ text: "Overview", link: "/guide/" });
  });

  it("lists every page of the guide, so a new page is added in one place", () => {
    const listed = new Set(guidePages(readme).map((p) => p.link));
    const pages = readdirSync(guide)
      .filter((f) => f.endsWith(".md") && f !== "README.md")
      .map((f) => `/guide/${f.replace(/\.md$/, "")}`);
    for (const page of pages)
      expect(listed, `${page} is missing in docs/guide/README.md`).toContain(page);
  });
});

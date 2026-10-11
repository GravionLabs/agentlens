import { describe, expect, it } from "vitest";
import { repoLink, type RepoFiles } from "../.vitepress/repo-links";

const present = new Map<string, "file" | "directory">([
  ["crates/agentlens-core/src/lib.rs", "file"],
  ["docs/adr/0003-desktop-app.md", "file"],
  ["docs/telemetry.md", "file"],
  ["docs/guide/modelling.md", "file"],
  ["crates/agentlens-core/tests/fixtures", "directory"],
]);
const files: RepoFiles = { kind: (p) => present.get(p) ?? null };
const repo = "https://github.com/GravionLabs/agentlens";

describe("repoLink", () => {
  it("leaves pages of the site, anchors, other sites and addresses of the site alone", () => {
    expect(repoLink("../telemetry.md#a", "guide/index.md", files)).toBe("../telemetry.md#a");
    expect(repoLink("modelling.md", "guide/index.md", files)).toBe("modelling.md");
    expect(repoLink("#top", "guide/index.md", files)).toBe("#top");
    expect(repoLink("https://example.com/x", "guide/index.md", files)).toBe(
      "https://example.com/x",
    );
    expect(repoLink("/app/", "guide/index.md", files)).toBe("/app/");
  });

  it("sends files and folders of the repository to GitHub", () => {
    expect(repoLink("../../crates/agentlens-core/src/lib.rs#L3", "guide/index.md", files)).toBe(
      `${repo}/blob/main/crates/agentlens-core/src/lib.rs#L3`,
    );
    expect(repoLink("../adr/0003-desktop-app.md", "guide/index.md", files)).toBe(
      `${repo}/blob/main/docs/adr/0003-desktop-app.md`,
    );
    expect(repoLink("../../crates/agentlens-core/tests/fixtures/", "guide/index.md", files)).toBe(
      `${repo}/tree/main/crates/agentlens-core/tests/fixtures`,
    );
  });

  it("leaves a link to nothing alone, so the dead-link check can fail the build", () => {
    expect(repoLink("missing.md", "guide/index.md", files)).toBe("missing.md");
    expect(repoLink("../../../outside.md", "guide/index.md", files)).toBe("../../../outside.md");
  });
});

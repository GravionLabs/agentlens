import { describe, expect, it } from "vitest";
import config from "../.vitepress/config.mts";

describe("site config", () => {
  it("keeps the decisions and the style guide off the site", () => {
    expect(config.srcExclude).toEqual(["adr/**", "STYLEGUIDE.md"]);
  });
});

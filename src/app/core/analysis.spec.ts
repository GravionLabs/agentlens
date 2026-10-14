import { describe, expect, it } from "vitest";
import { messageOf } from "./analysis";
import { FiltersService } from "./filters.service";

describe("filters", () => {
  it("turns the bar state into the payload of the commands", () => {
    const f = new FiltersService();
    f.setSince("7d");
    f.setProject("  /work/demo ");
    f.toggleSource("opencode");
    expect(f.payload()).toEqual({
      since: "7d",
      until: null,
      sources: ["opencode"],
      project: "/work/demo",
    });
    f.toggleSource("opencode");
    f.setSince("");
    f.setProject("");
    expect(f.payload()).toEqual({ since: null, until: null, sources: [], project: null });
  });
});

describe("messageOf", () => {
  it("reads strings, errors and their causes", () => {
    expect(messageOf("cannot parse `soon`")).toBe("cannot parse `soon`");
    expect(messageOf(new Error("boom"))).toBe("boom");
    expect(messageOf(new Error("outer", { cause: "inner" }))).toBe("inner");
    expect(messageOf(42)).toBe("Something went wrong.");
  });
});

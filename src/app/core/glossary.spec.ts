import { describe, expect, it } from "vitest";
import { GLOSSARY, GLOSSARY_GROUPS, glossaryEntry, searchGlossary } from "./glossary";

describe("glossary", () => {
  it("has unique ids and only known groups", () => {
    const ids = GLOSSARY.map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const e of GLOSSARY) expect(GLOSSARY_GROUPS).toContain(e.group);
  });

  it("explains overhead", () => {
    expect(glossaryEntry("overhead")?.short).toContain("requests");
  });

  it("searches terms and texts, case-insensitively", () => {
    expect(searchGlossary("OVERHEAD").some((e) => e.id === "overhead")).toBe(true);
    expect(searchGlossary("")).toHaveLength(GLOSSARY.length);
    expect(searchGlossary("zzzz")).toEqual([]);
  });
});

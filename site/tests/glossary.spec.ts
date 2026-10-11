import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { GLOSSARY, GLOSSARY_GROUPS } from "../../src/app/core/glossary";
import { renderGlossary } from "../glossary";

const page = resolve(__dirname, "../../docs/guide/glossary.md");

describe("the glossary page of the guide", () => {
  it("is made from the entries of the application", () => {
    const text = renderGlossary();
    for (const entry of GLOSSARY) {
      expect(text, entry.id).toContain(`### ${entry.term}`);
      expect(text, entry.id).toContain(entry.long);
    }
    for (const group of GLOSSARY_GROUPS) expect(text).toContain(`## ${group}`);
  });

  it("is up to date (run `pnpm docs:glossary` after changing a term)", () => {
    const text = renderGlossary();
    if (process.env["UPDATE_GLOSSARY"]) writeFileSync(page, text);
    expect(readFileSync(page, "utf8")).toBe(text);
  });
});

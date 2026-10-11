import { describe, expect, it } from "vitest";
import type { Answer, SkillsData } from "../../core/types";
import samples from "../../core/samples/skills.json";
import { overhead } from "./skills";

describe("skill overhead", () => {
  it("adds baseline, load and carry like the CLI", () => {
    const skills = (samples as unknown as Answer<SkillsData>).data.report.skills;
    const demo = skills.find((s) => s.name === "demo")!;
    expect(overhead(demo)).toBe(
      demo.baseline.tokens.count + demo.load.count + demo.carry.tokens.count,
    );
    expect(overhead(demo)).toBe(24);
  });
});

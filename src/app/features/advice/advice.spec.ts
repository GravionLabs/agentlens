import { describe, expect, it } from "vitest";
import type { Finding } from "../../core/types";
import { findingTitle, findingTotal } from "./advice";

const finding = (over: Partial<Finding>): Finding => ({
  kind: "unused-skills",
  explanation: "",
  action: "",
  count: 2,
  total: { count: 12_235_734, measured: false },
  share_of_baseline: 0.3014,
  share_of_total_tokens: null,
  items: [],
  notes: [],
  ...over,
});

describe("findings", () => {
  it("have a title for every kind", () => {
    expect(findingTitle("unused-skills")).toBe("Unused skills");
    expect(findingTitle("large-context")).toBe("Work on a large context");
    expect(findingTitle("context-size")).toBe("Context size");
  });

  it("say what the sum is, and mark an estimate", () => {
    expect(findingTotal(finding({}))).toBe(
      "Together they cost ~12,235,734 tokens, 30% of the baseline.",
    );
    expect(findingTotal(finding({ kind: "costly-descriptions", share_of_baseline: null }))).toBe(
      "Together they could save ~12,235,734 tokens.",
    );
    expect(
      findingTotal(finding({ total: { count: 5, measured: true }, share_of_baseline: null })),
    ).toBe("Together they cost 5 tokens.");
    expect(findingTotal(finding({ share_of_total_tokens: 0.0035 }))).toBe(
      "Together they cost ~12,235,734 tokens, 30% of the baseline, 0.35% of all tokens.",
    );
    expect(findingTotal(finding({ total: null }))).toBeNull();
  });
});

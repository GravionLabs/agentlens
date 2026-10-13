import { afterEach, describe, expect, it } from "vitest";
import { tokenFormat } from "../../core/token-format.service";
import type { Answer, CompareReport } from "../../core/types";
import samples from "../../core/samples/compare.json";
import { METRICS } from "./compare";

describe("compare metrics", () => {
  const sources = (samples as unknown as Answer<CompareReport>).data.sources;

  it("show values a client does not record as n/a, never as 0", () => {
    const claude = sources.find((s) => s.source === "claude-code")!;
    const rate = METRICS.find((m) => m.label === "Tool error rate")!;
    expect(rate.value({ ...claude, tool_error_rate: null })).toBe("n/a");
    const requests = METRICS.find((m) => m.label === "Requests")!;
    expect(requests.value({ ...claude, requests: null })).toBe("n/a");
    expect(requests.value({ ...claude, requests: 3 })).toBe("3");
  });

  it("mark the skill overhead as an estimate", () => {
    const m = METRICS.find((x) => x.label === "Skill overhead")!;
    expect(m.estimated).toBe(true);
    expect(
      m.value({ ...sources[0], skill_overhead_tokens: { count: 1234, measured: false } }),
    ).toBe("~1,234");
    expect(m.value({ ...sources[0], skill_overhead_tokens: null })).toBe("n/a");
    expect(m.value({ ...sources[0], skill_overhead_tokens: { count: 5, measured: true } })).toBe(
      "5",
    );
  });
});

describe("compare token metrics", () => {
  const s = (samples as unknown as Answer<CompareReport>).data.sources[0];
  afterEach(() => tokenFormat.set("full"));

  it("round with the setting and expose the exact count", () => {
    const m = METRICS.find((x) => x.label === "Input tokens")!;
    expect(m.value({ ...s, input_tokens: 2_114_305 })).toBe("2,114,305");
    tokenFormat.set("rounded");
    expect(m.value({ ...s, input_tokens: 2_114_305 })).toBe("2.11 M");
    expect(m.exact?.({ ...s, input_tokens: 2_114_305 })).toBe(2_114_305);
    expect(m.value({ ...s, input_tokens: null })).toBe("n/a");
  });
});

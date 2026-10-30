import { describe, expect, it } from "vitest";
import type {
  AdviceReport,
  Answer,
  CliTools,
  CompareReport,
  ContextReport,
  CostData,
  EfficiencyReport,
  FlowData,
  SessionSummary,
  SkillsData,
  SourceOverview,
  Timeline,
  TrendReport,
  UnusedReport,
} from "./types";
import advice from "./samples/advice.json";
import cliTools from "./samples/cli_tools.json";
import compare from "./samples/compare.json";
import context from "./samples/context.json";
import cost from "./samples/cost.json";
import efficiency from "./samples/efficiency.json";
import flow from "./samples/flow.json";
import overview from "./samples/overview.json";
import sessions from "./samples/sessions.json";
import skills from "./samples/skills.json";
import timeline from "./samples/timeline.json";
import trend from "./samples/trend.json";
import unused from "./samples/unused.json";

// These samples are written by the Rust side (`UPDATE_SAMPLES=1 cargo test -p agentlens-app`).
// Assigning them to the types fails the build when the two sides drift apart.
describe("samples from the backend", () => {
  it("fit the TypeScript types", () => {
    const answers: Answer<unknown>[] = [
      overview as unknown as Answer<SourceOverview[]>,
      skills as unknown as Answer<SkillsData>,
      advice as unknown as Answer<AdviceReport>,
      context as unknown as Answer<ContextReport>,
      unused as unknown as Answer<UnusedReport>,
      flow as unknown as Answer<FlowData>,
      cost as unknown as Answer<CostData>,
      efficiency as unknown as Answer<EfficiencyReport>,
      cliTools as unknown as Answer<CliTools>,
      compare as unknown as Answer<CompareReport>,
      sessions as unknown as Answer<SessionSummary[]>,
      timeline as unknown as Answer<Timeline>,
      trend as unknown as Answer<TrendReport>,
    ];
    for (const a of answers) {
      expect(Object.keys(a).sort()).toEqual(["data", "errors", "schema_version", "warnings"]);
      expect(a.schema_version).toBeGreaterThanOrEqual(2);
    }
  });

  it("have the fields the views read", () => {
    const s = (skills as unknown as Answer<SkillsData>).data.report.skills[0];
    expect(s.baseline.tokens.measured).toBe(false);
    expect(Object.keys(s.carry).sort()).toEqual([
      "cache_read_tokens",
      "requests",
      "tokens",
      "uncached_tokens",
    ]);
    const t = (timeline as unknown as Answer<Timeline>).data;
    expect(t.markers[0].kind.type).toBe("compaction");
    expect(t.points.length).toBeGreaterThan(0);
    const c = (cost as unknown as Answer<CostData>).data;
    expect(c.what_if?.target).toBe("claude-haiku-4-5");
    const o = (overview as unknown as Answer<SourceOverview[]>).data;
    expect(o.some((x) => !x.found)).toBe(true);
  });
});

describe("cli tools sample", () => {
  it("never offers the action", () => {
    expect((cliTools as unknown as Answer<CliTools>).data.offered).toBe(false);
  });
});

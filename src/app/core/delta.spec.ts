import { describe, expect, it } from "vitest";
import { deltaValue, deltaView, sumDeltas, trendNote } from "./delta";
import type { Delta, TrendWindows } from "./types";

const d = (over: Partial<Delta>): Delta => ({
  before: 10,
  after: 12,
  change: 2,
  change_pct: 20,
  measured: true,
  ...over,
});

describe("deltaView", () => {
  it("shows the arrow, the absolute and the relative change", () => {
    expect(deltaView(d({}), "count")).toEqual({
      direction: "up",
      arrow: "▲",
      text: "+2 (+20.0%)",
      tone: "neutral",
    });
    expect(deltaView(d({ change: -2, change_pct: -20 }), "count").text).toBe("-2 (-20.0%)");
  });

  it("colors only where a direction is good or bad", () => {
    expect(deltaView(d({}), "usd", "down").tone).toBe("bad");
    expect(deltaView(d({ change: -2, change_pct: -20 }), "usd", "down").tone).toBe("good");
    expect(deltaView(d({}), "ratio", "up").tone).toBe("good");
    expect(deltaView(d({}), "count", null).tone).toBe("neutral");
    expect(deltaView(d({ change: 0, change_pct: 0 }), "usd", "down")).toMatchObject({
      direction: "flat",
      tone: "neutral",
    });
  });

  it("is n/a where a window is empty and where the earlier value is 0", () => {
    expect(deltaView(d({ change: null, change_pct: null }), "count", "down")).toEqual({
      direction: "none",
      arrow: "",
      text: "n/a",
      tone: "neutral",
    });
    expect(deltaView(d({ before: 0, change_pct: null }), "count").text).toBe("+2 (n/a)");
  });

  it("marks estimates with a tilde", () => {
    expect(deltaView(d({ change: 1.5, change_pct: 15, measured: false }), "usd").text).toBe(
      "+~$1.50 (+~15.0%)",
    );
  });

  it("writes shares as points and per-day counts with a decimal", () => {
    expect(deltaView(d({ change: 0.052, change_pct: 6 }), "ratio", "up").text).toBe(
      "+5.2 pts (+6.0%)",
    );
    expect(deltaView(d({ change: 0.5, change_pct: 50 }), "count", null, true).text).toBe(
      "+0.5 (+50.0%)",
    );
  });
});

describe("deltaValue", () => {
  it("formats values and says n/a for none", () => {
    expect(deltaValue(null, "usd")).toBe("n/a");
    expect(deltaValue(1.5, "usd", true)).toBe("~$1.50");
    expect(deltaValue(0.456, "ratio")).toBe("45.6%");
    expect(deltaValue(0.25, "count", false, true)).toBe("0.3");
  });
});

const windows = (current: number, against: number, normalized = false): TrendWindows => ({
  current: { since: "2026-10-16T00:00:00Z", until: "2026-10-30T00:00:00Z", sessions: current },
  against: { since: "2026-10-02T00:00:00Z", until: "2026-10-16T00:00:00Z", sessions: against },
  normalized,
});

describe("sumDeltas", () => {
  it("adds the figures and derives the change", () => {
    const sum = sumDeltas(
      [d({ before: 2, after: 5 }), d({ before: 2, after: 1, measured: false })],
      windows(1, 1),
    );
    expect(sum).toEqual({ before: 4, after: 6, change: 2, change_pct: 50, measured: false });
  });

  it("has no change when a window is empty or the earlier sum is 0", () => {
    expect(sumDeltas([d({})], windows(0, 3)).change).toBeNull();
    expect(sumDeltas([], windows(2, 3))).toMatchObject({ change: 0, change_pct: null });
  });
});

describe("trendNote", () => {
  it("names the comparison window and says when counts are per day", () => {
    expect(trendNote(windows(1, 2))).toBe(
      "vs. previous period (2026-10-02 – 2026-10-16, 2 sessions)",
    );
    expect(trendNote(windows(1, 2, true))).toContain("per day");
  });
});

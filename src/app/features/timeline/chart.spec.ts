import { describe, expect, it } from "vitest";
import type { TimelinePoint } from "../../core/types";
import {
  HEIGHT,
  MARGIN,
  WIDTH,
  areaPath,
  linePath,
  markerLabel,
  markerX,
  scales,
  yTicks,
} from "./chart";

const point = (request: number, tokens: number): TimelinePoint => ({
  request,
  at: null,
  context_tokens: tokens,
  output_tokens: 0,
  cost_usd: null,
  cost_estimated: false,
});

describe("timeline chart", () => {
  const points = [point(0, 1000), point(1, 4000), point(2, 2000)];
  const s = scales(points);

  it("maps requests and tokens into the plot area, peak on top", () => {
    expect(s.x(0)).toBe(MARGIN.left);
    expect(s.x(2)).toBe(WIDTH - MARGIN.right);
    expect(s.y(4000)).toBe(MARGIN.top);
    expect(s.y(0)).toBe(HEIGHT - MARGIN.bottom);
    expect(s.max).toBe(4000);
  });

  it("draws the line through all points and closes the area on the axis", () => {
    expect(linePath(points, s).split("L")).toHaveLength(3);
    expect(areaPath(points, s).endsWith("Z")).toBe(true);
    expect(areaPath([], s)).toBe("");
  });

  it("places a marker in the gap before its request", () => {
    expect(markerX({ request: 1, at: null, kind: { type: "compaction" } }, s, 0)).toBe(s.x(0.5));
    // A marker before the first request stays on the axis.
    expect(markerX({ request: 0, at: null, kind: { type: "compaction" } }, s, 0)).toBe(s.x(0));
  });

  it("labels markers and rounds ticks", () => {
    expect(markerLabel({ type: "skill", name: "demo", trigger: "user" })).toBe("skill demo (user)");
    expect(markerLabel({ type: "permission-prompt", wait_ms: 1500 })).toBe(
      "permission prompt (1.5 s)",
    );
    expect(markerLabel({ type: "permission-prompt", wait_ms: null })).toBe("permission prompt");
    expect(markerLabel({ type: "model-switch", from: "opus", to: "haiku" })).toBe(
      "model switch: opus to haiku",
    );
    expect(yTicks(4000)).toEqual([0, 2000, 4000]);
  });

  it("copes with a single request", () => {
    const one = scales([point(0, 10)]);
    expect(Number.isFinite(one.x(0)) && Number.isFinite(one.y(10))).toBe(true);
  });
});

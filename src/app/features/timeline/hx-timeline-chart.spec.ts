// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import type { Timeline } from "../../core/types";
import type { ColorContext } from "../../ui/hatch";
import {
  buildTimelineData,
  chartOptions,
  contextPoints,
  fallbackMarkers,
  fallbackRows,
  markerLines,
  markerPoints,
  pointLines,
  syntheticTimeline,
  thinnedText,
  titleText,
} from "./hx-timeline-chart";

const t: Timeline = {
  session: {
    source: "claude-code",
    id: "s",
    project: null,
    model: null,
    started_at: null,
    ended_at: null,
    requests: 3,
    peak_tokens: 4000,
  } as Timeline["session"],
  points: [
    {
      request: 0,
      at: null,
      context_tokens: 1000,
      output_tokens: 10,
      cost_usd: null,
      cost_estimated: false,
    },
    {
      request: 1,
      at: null,
      context_tokens: 4000,
      output_tokens: 20,
      cost_usd: 0.5,
      cost_estimated: true,
    },
    {
      request: 2,
      at: null,
      context_tokens: 2000,
      output_tokens: 30,
      cost_usd: 0.25,
      cost_estimated: false,
    },
  ],
  markers: [
    { request: 0, at: null, kind: { type: "skill", name: "review", trigger: "user" } },
    { request: 2, at: null, kind: { type: "compaction" } },
    { request: 2, at: null, kind: { type: "tool-error", tool: "Bash" } },
  ],
  thinned_from: null,
};

const ctx = (dataIndex = 0): ColorContext => ({
  dataIndex,
  chart: { ctx: { createPattern: () => null } },
});
const paint = (_c: ColorContext, color: string, estimated: boolean) =>
  estimated ? `hatch:${color}` : color;

describe("buildTimelineData", () => {
  const data = buildTimelineData(t, paint);

  it("has the filled context line with 1-based x and one dataset per marker kind that occurs", () => {
    expect(data.datasets.map((d) => d["label"])).toEqual([
      "context size",
      "skill loaded",
      "compaction",
      "tool error",
    ]);
    expect(contextPoints(t)).toEqual([
      { x: 1, y: 1000 },
      { x: 2, y: 4000 },
      { x: 3, y: 2000 },
    ]);
    expect(data.datasets[0]["fill"]).toBe("origin");
  });

  it("puts markers in the gap before their request, clamped to the first request, at the peak", () => {
    const skill = markerPoints(t, "skill", 4000);
    expect(skill.map((p) => [p.x, p.y])).toEqual([[1, 4000]]);
    const compaction = markerPoints(t, "compaction", 4000);
    expect(compaction.map((p) => p.x)).toEqual([2.5]);
  });

  it("paints the area solid when measured and hatched when estimated", () => {
    const fill = data.datasets[0]["backgroundColor"] as (c: ColorContext) => unknown;
    expect(String(fill(ctx()))).toContain("color-mix");
    const hatched = buildTimelineData(t, paint, true).datasets[0]["backgroundColor"] as (
      c: ColorContext,
    ) => unknown;
    expect(hatched(ctx())).toBe("hatch:var(--accent)");
  });

  it("handles a session without points or markers", () => {
    const empty = buildTimelineData({ ...t, points: [], markers: [] }, paint);
    expect(empty.datasets).toHaveLength(1);
    expect(empty.datasets[0]["data"]).toEqual([]);
  });
});

describe("tooltip text", () => {
  it("gives size, output and cost with a tilde for estimates", () => {
    expect(pointLines(t, 1)).toEqual([
      "4.0k tokens in context (measured)",
      "20 tokens out",
      "cost ~$0.50",
    ]);
    expect(pointLines(t, 0)).toHaveLength(2);
    expect(pointLines(t, 9)).toEqual([]);
  });

  it("titles points and markers", () => {
    expect(titleText(false, 2)).toBe("Request 2");
    expect(titleText(true, 2.5)).toBe("Before request 3");
  });

  it("says how a thinned timeline was reduced", () => {
    expect(thinnedText(t)).toBeNull();
    expect(thinnedText({ ...t, thinned_from: 2000 })).toBe(
      "thinned from 2,000 to 3 points (peaks are kept)",
    );
  });
});

describe("chartOptions", () => {
  const o = chartOptions({ ...t, thinned_from: 2000 }) as {
    scales: {
      x: { min: number; max: number; ticks: { callback(v: number): string } };
      y: { ticks: { callback(v: number): string } };
    };
    plugins: { tooltip: { callbacks: { footer(): string; label(i: unknown): unknown } } };
  };

  it("spans the requests and writes compact y ticks", () => {
    expect([o.scales.x.min, o.scales.x.max]).toEqual([1, 3]);
    expect(o.scales.y.ticks.callback(2_500_000)).toBe("2.5M");
  });

  it("labels only whole request numbers on x", () => {
    expect(o.scales.x.ticks.callback(2)).toBe("2");
    expect(o.scales.x.ticks.callback(1.5)).toBe("");
  });

  it("shows thinned_from in the tooltip footer and the marker label for marker points", () => {
    expect(o.plugins.tooltip.callbacks.footer()).toContain("thinned from 2,000");
    const marker = markerPoints(t, "tool-error", 1)[0];
    expect(
      o.plugins.tooltip.callbacks.label({
        dataset: { kind: "tool-error" },
        raw: marker,
        dataIndex: 0,
      }),
    ).toBe("tool error: Bash");
  });
});

describe("chartOptions edge cases", () => {
  type O = {
    scales: { x: { min: number; max: number } };
    plugins: { tooltip: { callbacks: { title(i: unknown): string; footer(): string } } };
  };
  it("keeps a valid x range for no points and for a single point", () => {
    const none = chartOptions({ ...t, points: [], markers: [] }) as O;
    expect([none.scales.x.min, none.scales.x.max]).toEqual([1, 2]);
    const one = chartOptions({ ...t, points: [t.points[0]] }) as O;
    expect([one.scales.x.min, one.scales.x.max]).toEqual([1, 2]);
  });

  it("titles points with the request and markers with the gap, and has an empty footer when not thinned", () => {
    const o = chartOptions(t) as O;
    const cb = o.plugins.tooltip.callbacks;
    expect(cb.title([{ dataset: {}, raw: { x: 2, y: 1 } }])).toBe("Request 2");
    expect(cb.title([{ dataset: { kind: "compaction" }, raw: { x: 2.5, y: 1 } }])).toBe(
      "Before request 3",
    );
    expect(cb.title([])).toBe("Request 0");
    expect(cb.footer()).toBe("");
  });
});

describe("fallback", () => {
  it("lists points and markers as text", () => {
    expect(fallbackRows(t)[1]).toEqual({
      request: "2",
      cells: ["4.0k", "20", "~$0.50"],
    });
    expect(fallbackRows(t)[0].cells[2]).toBe("n/a");
    expect(fallbackMarkers(t)).toContain("before request 3: compaction");
  });
});

describe("markerLines", () => {
  it("draws one full-height line per marker point, compaction solid and thick, others dashed and faint", () => {
    document.documentElement.style.setProperty("--bad", "#aa0000");
    document.documentElement.style.setProperty("--c-load", "#0000aa");
    const strokes: {
      style: unknown;
      alpha: number;
      width: number;
      dash: number[];
      path: string;
    }[] = [];
    let dash: number[] = [];
    let path = "";
    let saves = 0;
    const g = {
      save: () => saves++,
      restore: () => saves--,
      beginPath: () => (path = ""),
      moveTo: (x: number, y: number) => (path += `M${x},${y}`),
      lineTo: (x: number, y: number) => (path += `L${x},${y}`),
      setLineDash: (d: number[]) => (dash = d),
      stroke: () =>
        strokes.push({
          style: g.strokeStyle,
          alpha: g.globalAlpha,
          width: g.lineWidth,
          dash,
          path,
        }),
      strokeStyle: "" as unknown,
      globalAlpha: 1,
      lineWidth: 1,
    };
    const canvas = document.createElement("canvas");
    document.body.append(canvas);
    markerLines.beforeDatasetsDraw({
      ctx: g as unknown as CanvasRenderingContext2D,
      canvas,
      chartArea: { top: 10, bottom: 200 },
      data: { datasets: [{}, { kind: "compaction" }, { kind: "skill" }] },
      getDatasetMeta: (i) => ({ data: i === 1 ? [{ x: 50 }, { x: 80 }] : [{ x: 20 }] }),
    });
    expect(strokes).toEqual([
      { style: "#aa0000", alpha: 1, width: 2.5, dash: [], path: "M50,10L50,200" },
      { style: "#aa0000", alpha: 1, width: 2.5, dash: [], path: "M80,10L80,200" },
      { style: "#0000aa", alpha: 0.55, width: 1, dash: [4, 3], path: "M20,10L20,200" },
    ]);
    expect(saves).toBe(0);
  });
});

describe("syntheticTimeline", () => {
  it("makes n points with all five marker kinds", () => {
    const s = syntheticTimeline(t, 40);
    expect(s.points).toHaveLength(40);
    expect(new Set(s.markers.map((m) => m.kind.type))).toEqual(
      new Set(["skill", "compaction", "tool-error", "hook-failed", "permission-prompt"]),
    );
    expect(s.thinned_from).toBe(200);
  });

  it("handles n of 1 without markers", () => {
    const s = syntheticTimeline(t, 1);
    expect(s.points).toHaveLength(1);
    expect(s.markers).toEqual([]);
  });
});

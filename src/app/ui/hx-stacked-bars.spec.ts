import { describe, expect, it, vi } from "vitest";
import {
  buildChartData,
  chartOptions,
  clip,
  fallbackRows,
  tooltipLine,
  valueText,
} from "./hx-stacked-bars";
import type { ColorContext } from "./hatch";
import type { BarRow } from "./stacked-bars";

const rows: BarRow[] = [
  {
    label: "alpha",
    total: "~2.3M",
    segments: [
      { label: "baseline", value: 200, color: "var(--a)", estimated: true },
      { label: "load", value: 2_100_000, color: "var(--b)", estimated: false },
    ],
  },
  {
    label: "a-very-long-skill-name-over-the-limit",
    total: "n/a",
    segments: [
      { label: "baseline", value: 0, color: "var(--a)", estimated: false },
      { label: "load", value: 1_500, color: "var(--b)", estimated: true },
    ],
  },
];

const ctx = (dataIndex: number): ColorContext => ({
  dataIndex,
  chart: { ctx: { createPattern: () => null } },
});

describe("buildChartData", () => {
  const paint = vi.fn((_c: ColorContext, color: string, estimated: boolean) =>
    estimated ? `hatch:${color}` : color,
  );
  const data = buildChartData(rows, paint);

  it("makes one dataset per segment name with values per bar", () => {
    expect(data.datasets.map((d) => d.label)).toEqual(["baseline", "load"]);
    expect(data.datasets[1].data).toEqual([2_100_000, 1_500]);
  });

  it("paints estimated bars with the pattern and measured ones solid", () => {
    expect(data.datasets[0].backgroundColor(ctx(0))).toBe("hatch:var(--a)");
    expect(data.datasets[0].backgroundColor(ctx(1))).toBe("var(--a)");
    expect(data.datasets[1].backgroundColor(ctx(0))).toBe("var(--b)");
    expect(data.datasets[1].backgroundColor(ctx(1))).toBe("hatch:var(--b)");
    expect(data.datasets[1].borderColor(ctx(1))).toBe("var(--b)");
  });

  it("clips long labels at 26 characters", () => {
    expect(data.labels[0]).toBe("alpha");
    expect(data.labels[1]).toBe("a-very-long-skill-name-ov…");
    expect(clip("x".repeat(26))).toBe("x".repeat(26));
    expect(clip("x".repeat(27))).toHaveLength(26);
  });

  it("handles no rows", () => {
    expect(buildChartData([], paint)).toEqual({ labels: [], datasets: [] });
  });
});

describe("text", () => {
  it("marks estimates with a tilde and uses compact numbers", () => {
    expect(valueText(2_100_000, true)).toBe("~2.1M");
    expect(valueText(1_500, false)).toBe("1.5k");
    expect(valueText(7, false)).toBe("7");
  });

  it("writes the tooltip line like the SVG title", () => {
    expect(tooltipLine("load", 2_100_000, true)).toBe("load: ~2.1M (estimated)");
    expect(tooltipLine("carry", 900, false)).toBe("carry: 900 (measured)");
  });

  it("builds the fallback table rows", () => {
    expect(fallbackRows(rows)[0]).toEqual({
      label: "alpha",
      cells: ["baseline ~200", "load 2.1M"],
      total: "~2.3M",
    });
    expect(fallbackRows([])).toEqual([]);
  });
});

describe("chartOptions", () => {
  const o = chartOptions(rows) as {
    indexAxis: string;
    scales: { x: { stacked: boolean; ticks: { callback: (v: number) => string } } };
    plugins: {
      tooltip: {
        callbacks: {
          title: (i: { dataIndex: number }[]) => string;
          label: (i: unknown) => string;
          footer: (i: { dataIndex: number }[]) => string;
        };
      };
    };
  };

  it("is horizontal and stacked with compact ticks", () => {
    expect(o.indexAxis).toBe("y");
    expect(o.scales.x.stacked).toBe(true);
    expect(o.scales.x.ticks.callback(2_100_000)).toBe("2.1M");
  });

  it("shows the full label, the estimate flag and the total in the tooltip", () => {
    const cb = o.plugins.tooltip.callbacks;
    expect(cb.title([{ dataIndex: 1 }])).toBe("a-very-long-skill-name-over-the-limit");
    const item = {
      dataset: { label: "load", estimated: [false, true] },
      dataIndex: 1,
      raw: 1_500,
    };
    expect(cb.label(item)).toBe("load: ~1.5k (estimated)");
    expect(cb.footer([{ dataIndex: 0 }])).toBe("total: ~2.3M");
    expect(cb.footer([])).toBe("total: ~2.3M");
  });
});

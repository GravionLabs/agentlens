import { Chart } from "chart.js";
import { describe, expect, it } from "vitest";
import { loadChartJs } from "./chart-loader";

const ids = (registry: { items: Record<string, unknown> }) => Object.keys(registry.items).sort();

describe("loadChartJs", () => {
  it("returns the Chart constructor as the default export", async () => {
    const module = await loadChartJs();
    expect(module.default).toBe(Chart);
  });

  it("registers only the bar and line parts", async () => {
    await loadChartJs();
    expect(ids(Chart.registry.controllers)).toEqual(["bar", "line"]);
    expect(ids(Chart.registry.elements)).toEqual(["bar", "line", "point"]);
    expect(ids(Chart.registry.scales)).toEqual(["category", "linear"]);
    expect(ids(Chart.registry.plugins)).toEqual(["filler", "legend", "tooltip"]);
  });

  it("can run twice without registering anything more", async () => {
    await loadChartJs();
    await loadChartJs();
    expect(ids(Chart.registry.controllers)).toEqual(["bar", "line"]);
  });
});

// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { signal } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { type BarRow, StackedBars } from "./stacked-bars";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

const rows: BarRow[] = [
  {
    label: "alpha",
    total: "~2.3M",
    segments: [
      { label: "baseline", value: 200_000, color: "var(--a)", estimated: true },
      { label: "load", value: 1_000_000, color: "var(--b)", estimated: false },
    ],
  },
  {
    label: "a-very-long-skill-name-over-the-limit",
    total: "n/a",
    segments: [
      { label: "baseline", value: 0, color: "var(--a)", estimated: false },
      { label: "load", value: 500_000, color: "var(--b)", estimated: true },
    ],
  },
];

/** Signal inputs cannot be bound in this JIT setup, so the fields are replaced by plain signals before the first render. */
function render(data: BarRow[], label?: string): HTMLElement {
  const fixture = TestBed.createComponent(StackedBars);
  const inputs = fixture.componentInstance as unknown as Record<string, unknown>;
  inputs["rows"] = signal(data);
  if (label) inputs["label"] = signal(label);
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

describe("StackedBars", () => {
  it("draws one rect per non-empty segment and skips zero-width ones", () => {
    const el = render(rows);
    expect(el.querySelectorAll("svg rect[height='16']").length).toBe(3);
  });

  it("scales segments against the largest row total", () => {
    const rects = render(rows).querySelectorAll("svg rect[height='16']");
    // plot is 660 units wide; the largest total is 1.2M, so 1M is 550 units and 200k is 110
    expect(Number(rects[0].getAttribute("width"))).toBeCloseTo(110, 5);
    expect(Number(rects[1].getAttribute("width"))).toBeCloseTo(550, 5);
  });

  it("hatches estimated segments with a pattern and fills measured ones solid", () => {
    const rects = render(rows).querySelectorAll("svg rect[height='16']");
    expect(rects[0].getAttribute("fill")).toMatch(/^url\(#hatch-var/);
    expect(rects[1].getAttribute("fill")).toBe("var(--b)");
    expect(rects[2].getAttribute("fill")).toMatch(/^url\(#hatch-/);
  });

  it("defines one hatch pattern per distinct color", () => {
    const ids = [...render(rows).querySelectorAll("pattern")].map((p) => p.id);
    expect(ids).toEqual(["hatch-vara", "hatch-varb"]);
  });

  it("tells estimated from measured in the tooltip, with a tilde for estimates", () => {
    const titles = [...render(rows).querySelectorAll("rect title")].map((t) =>
      t.textContent?.replace(/\s+/g, " ").trim(),
    );
    expect(titles[0]).toBe("alpha – baseline: ~200k (estimated)");
    expect(titles[1]).toBe("alpha – load: 1.0M (measured)");
  });

  it("shows the row totals and clips long labels", () => {
    const texts = [...render(rows).querySelectorAll("svg > text")].map((t) =>
      t.textContent?.trim(),
    );
    expect(texts).toContain("~2.3M");
    expect(texts).toContain("n/a");
    expect(texts).toContain("alpha");
    expect(texts).toContain("a-very-long-skill-name-ov…");
  });

  it("exposes the accessible label as an image", () => {
    const svg = render(rows, "Skill overhead").querySelector("svg")!;
    expect(svg.getAttribute("role")).toBe("img");
    expect(svg.getAttribute("aria-label")).toBe("Skill overhead");
  });

  it("uses a default label and draws nothing for no rows", () => {
    const el = render([]);
    expect(el.querySelector("svg")?.getAttribute("aria-label")).toBe("Bar chart");
    expect(el.querySelectorAll("rect[height='16']").length).toBe(0);
  });

  it("does not divide by zero when every value is zero", () => {
    const el = render([
      {
        label: "z",
        total: "0",
        segments: [{ label: "x", value: 0, color: "red", estimated: false }],
      },
    ]);
    expect(el.querySelectorAll("rect[height='16']").length).toBe(0);
  });
});

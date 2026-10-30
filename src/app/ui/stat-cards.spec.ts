// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { type Stat, StatCards, trendStat } from "./stat-cards";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

/** The vitest setup compiles in JIT mode, which cannot bind signal inputs, so the input is replaced by a plain signal-like getter. */
function render(stats: Stat[]): HTMLElement {
  const fixture = TestBed.createComponent(StatCards);
  (fixture.componentInstance as unknown as { stats: () => Stat[] }).stats = () => stats;
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

describe("StatCards", () => {
  it("shows values verbatim, keeping the tilde of an estimate and n/a", () => {
    const el = render([
      { value: "~1.2k", label: "Overhead" },
      { value: "n/a", label: "Cost" },
    ]);
    const cards = el.querySelectorAll("helix-stat-card");
    expect(cards.length).toBe(2);
    expect(cards[0].querySelector(".helix-stat-card__value")?.textContent?.trim()).toBe("~1.2k");
    expect(cards[0].querySelector(".helix-stat-card__label")?.textContent?.trim()).toBe("Overhead");
    expect(cards[1].querySelector(".helix-stat-card__value")?.textContent?.trim()).toBe("n/a");
  });

  it("sets the tooltip only when given", () => {
    const el = render([
      { value: "1k", label: "A", title: "1,234 tokens" },
      { value: "2", label: "B" },
    ]);
    const cards = el.querySelectorAll("helix-stat-card");
    expect(cards[0].getAttribute("title")).toBe("1,234 tokens");
    expect(cards[1].hasAttribute("title")).toBe(false);
  });

  it("shows the change under a card, colored only by its tone", () => {
    const el = render([
      {
        value: "$3.00",
        label: "Cost",
        delta: { direction: "up", arrow: "\u25B2", text: "+$1.00 (+50.0%)", tone: "bad" },
      },
      {
        value: "9",
        label: "Neutral",
        delta: { direction: "down", arrow: "\u25BC", text: "-1 (-10.0%)", tone: "neutral" },
      },
      { value: "1", label: "Plain" },
    ]);
    const deltas = el.querySelectorAll(".delta");
    expect(deltas).toHaveLength(2);
    expect(deltas[0].textContent).toContain("+$1.00 (+50.0%)");
    expect(deltas[0].classList.contains("bad")).toBe(true);
    expect(deltas[1].classList.contains("bad") || deltas[1].classList.contains("good")).toBe(false);
  });

  it("builds a trend stat with the current value and n/a for an empty window", () => {
    const empty = { before: null, after: null, change: null, change_pct: null, measured: true };
    const stat = trendStat("Cost", empty, "usd", { good: "down" });
    expect(stat.value).toBe("n/a");
    expect(stat.delta?.text).toBe("n/a");
    expect(trendStat("Requests", empty, "count", { perDay: true }).label).toBe("Requests per day");
  });

  it("renders nothing for an empty list", () => {
    expect(render([]).querySelectorAll("helix-stat-card").length).toBe(0);
  });
});

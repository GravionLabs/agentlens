// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { type Stat, StatCards } from "./stat-cards";

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

  it("renders nothing for an empty list", () => {
    expect(render([]).querySelectorAll("helix-stat-card").length).toBe(0);
  });
});

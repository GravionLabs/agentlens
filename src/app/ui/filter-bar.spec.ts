// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { FiltersService } from "../core/filters.service";
import { FilterBar } from "./filter-bar";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
beforeEach(() => localStorage.clear());
afterEach(() => {
  TestBed.resetTestingModule();
  document.querySelectorAll(".cdk-overlay-container").forEach((e) => e.remove());
});

async function render() {
  const fixture = TestBed.createComponent(FilterBar);
  fixture.detectChanges();
  await fixture.whenStable();
  return {
    fixture,
    el: fixture.nativeElement as HTMLElement,
    filters: TestBed.inject(FiltersService),
  };
}

const overlay = () => document.querySelector(".cdk-overlay-container") as HTMLElement;

describe("FilterBar", () => {
  it("labels each control and shows the current period", async () => {
    const { el } = await render();
    const period = el.querySelector<HTMLElement>("#filter-period")!;
    expect(el.querySelector('label[for="filter-period"]')!.textContent).toContain("Period");
    expect(period.getAttribute("role")).toBe("combobox");
    expect(period.textContent).toContain("Last 30 days");
    expect(el.querySelector('label[for="filter-sources"]')!.textContent).toContain("Sources");
    expect(el.querySelector('label[for="filter-window"]')!.textContent).toContain("Custom window");
  });

  it("sets the period from the listbox", async () => {
    const { fixture, el, filters } = await render();
    el.querySelector<HTMLElement>("#filter-period")!.click();
    fixture.detectChanges();
    await fixture.whenStable();
    const option = [...overlay().querySelectorAll<HTMLElement>('[role="option"]')].find((o) =>
      o.textContent!.includes("Last 7 days"),
    )!;
    option.click();
    fixture.detectChanges();
    expect(filters.since()).toBe("7d");
  });

  it("selects and clears sources through the multi-select", async () => {
    const { fixture, el, filters } = await render();
    el.querySelector<HTMLElement>("#filter-sources")!.click();
    fixture.detectChanges();
    await fixture.whenStable();
    const options = overlay().querySelectorAll<HTMLElement>('[role="option"]');
    expect(options).toHaveLength(4);
    [...options].find((o) => o.textContent!.includes("OpenCode"))!.click();
    fixture.detectChanges();
    expect(filters.sources()).toEqual(["opencode"]);
    [...options].find((o) => o.textContent!.includes("OpenCode"))!.click();
    fixture.detectChanges();
    expect(filters.sources()).toEqual([]);
  });

  it("keeps the project text input wired to the service", async () => {
    const { el, filters } = await render();
    const input = el.querySelector<HTMLInputElement>('input[placeholder^="Directory"]')!;
    input.value = "/work/demo";
    input.dispatchEvent(new Event("change"));
    expect(filters.project()).toBe("/work/demo");
  });

  it("renders the custom window picker as a field that opens a dialog", async () => {
    const { el } = await render();
    expect(el.querySelector("hx-date-time-range-picker")).not.toBeNull();
    expect(el.querySelector<HTMLInputElement>("#filter-window")!.readOnly).toBe(true);
  });
});

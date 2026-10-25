// @vitest-environment jsdom
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Glossary } from "./glossary";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

function render() {
  TestBed.configureTestingModule({ providers: [provideRouter([])] });
  const fixture = TestBed.createComponent(Glossary);
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement };
}

function type(fixture: ReturnType<typeof render>["fixture"], value: string) {
  const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>("input")!;
  input.value = value;
  input.dispatchEvent(new Event("input"));
  fixture.detectChanges();
}

describe("Glossary search", () => {
  it("uses the helix input with an accessible name", () => {
    const { el } = render();
    const input = el.querySelector("input")!;
    expect(input.hasAttribute("hx-input")).toBe(true);
    expect(input.getAttribute("aria-label")).toBe("Search terms");
  });

  it("lists every term when the query is empty", () => {
    const { el } = render();
    expect(el.querySelectorAll(".entry").length).toBeGreaterThan(0);
  });

  it("narrows the entries to the query", () => {
    const { fixture, el } = render();
    const all = el.querySelectorAll(".entry").length;
    type(fixture, "cache");
    const found = el.querySelectorAll(".entry").length;
    expect(found).toBeGreaterThan(0);
    expect(found).toBeLessThan(all);
  });

  it("says so when no term matches", () => {
    const { fixture, el } = render();
    type(fixture, "zzzz-no-such-term");
    expect(el.querySelectorAll(".entry")).toHaveLength(0);
    expect(el.textContent).toContain("No term matches.");
  });
});

// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { provideRouter } from "@angular/router";
import { FiltersService } from "../core/filters.service";
import { Ipc } from "../core/ipc";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { PageHeader } from "./page-header";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => {
  TestBed.resetTestingModule();
  document.querySelectorAll(".cdk-overlay-container").forEach((e) => e.remove());
});

function create(exportView?: string) {
  TestBed.configureTestingModule({
    providers: [provideRouter([]), { provide: Ipc, useValue: {} }],
  });
  const fixture = TestBed.createComponent(PageHeader);
  // The vitest setup compiles in JIT mode, which cannot bind signal inputs: replace them.
  Object.assign(fixture.componentInstance, {
    title: () => "Skills",
    summary: () => "What skills cost.",
    exportView: () => exportView,
    exportFormats: () => ["json"],
    exportOptions: () => undefined,
  });
  fixture.detectChanges();
  return fixture;
}

describe("PageHeader", () => {
  beforeEach(() => localStorage.clear());
  it("shows title as h2 and the summary", () => {
    const el = create().nativeElement as HTMLElement;
    expect(el.querySelector("h2")?.textContent?.trim()).toBe("Skills");
    expect(el.querySelector(".helix-page-header__subtitle")?.textContent?.trim()).toBe(
      "What skills cost.",
    );
    expect(el.querySelector("app-export-menu")).toBeNull();
    expect(el.querySelector(".details")).toBeNull();
  });

  it("folds the explanation open and closed with the Explain button", () => {
    const fixture = create();
    const el = fixture.nativeElement as HTMLElement;
    const btn = el.querySelector<HTMLButtonElement>("button.explain")!;
    expect(btn.textContent?.trim()).toBe("Explain");
    expect(btn.getAttribute("aria-expanded")).toBe("false");
    btn.click();
    fixture.detectChanges();
    expect(btn.textContent?.trim()).toBe("Hide");
    expect(btn.getAttribute("aria-expanded")).toBe("true");
    const details = el.querySelector(".details")!;
    expect(details.id).toBe(btn.getAttribute("aria-controls"));
    expect(details.querySelector("a.more")?.getAttribute("href")).toBe("/glossary");
    btn.click();
    fixture.detectChanges();
    expect(el.querySelector(".details")).toBeNull();
  });

  it("offers the export menu only for an export view", () => {
    const el = create("skills").nativeElement as HTMLElement;
    expect(el.querySelector("app-export-menu")).not.toBeNull();
  });

  it("shows the active window", () => {
    const fixture = create();
    const filters = TestBed.inject(FiltersService);
    const text = () =>
      fixture.nativeElement.querySelector('[data-testid="window"]').textContent.trim();
    expect(text()).toBe("last 30 days");
    filters.setLast(6, "h");
    fixture.detectChanges();
    expect(text()).toBe("last 6 hours");
  });
});

// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { provideRouter } from "@angular/router";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./app";
import { NAV, routes } from "./app.routes";
import { FiltersService } from "./core/filters.service";
import { LoadingService } from "./core/loading.service";
import { Ipc, SAMPLE_MODE } from "./core/ipc";
import { ZoomService } from "./core/zoom.service";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
beforeEach(() => {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
});
afterEach(() => {
  vi.unstubAllGlobals();
  TestBed.resetTestingModule();
  document.querySelectorAll(".cdk-overlay-container").forEach((e) => e.remove());
});

function render(invoke = vi.fn(async () => "0.1.0")) {
  TestBed.configureTestingModule({
    providers: [
      provideRouter([]),
      { provide: Ipc, useValue: { invoke } },
      { provide: SAMPLE_MODE, useValue: false },
    ],
  });
  const fixture = TestBed.createComponent(App);
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement, invoke };
}

describe("app", () => {
  it("has a router configuration", () => {
    expect(Array.isArray(routes)).toBe(true);
  });

  it("links every view in the navigation, in group order", () => {
    const { el } = render();
    const links = [...el.querySelectorAll<HTMLAnchorElement>("helix-nav-rail a[href]")].filter(
      (a) => a.getAttribute("href") !== "/",
    );
    const hrefs = links.map((a) => a.getAttribute("href"));
    expect(hrefs).toEqual(NAV.map((n) => `/${n.path}`));
    expect(links.map((a) => a.textContent?.trim())).toEqual(NAV.map((n) => n.label));
  });

  it("groups the navigation into Data, Optimize and Analyze, then Glossary", () => {
    const { el } = render();
    const sections = [
      ...el.querySelectorAll("helix-nav-rail .nav-section, helix-nav-rail [class*='section']"),
    ]
      .map((e) => e.textContent?.trim().toLowerCase())
      .filter((t) => ["data", "optimize", "analyze"].includes(t ?? ""));
    expect(sections).toEqual(["data", "optimize", "analyze"]);
    const members = (g: string) => NAV.filter((n) => n.group === g).map((n) => n.label);
    expect(members("Data")).toEqual(["Sources", "Timeline", "Flow"]);
    expect(members("Optimize")).toEqual(["Skills", "Unused", "Context", "Advice"]);
    expect(members("Analyze")).toEqual(["Cost", "Efficiency", "Compare"]);
    expect(NAV.filter((n) => !n.group).map((n) => n.label)).toEqual(["Glossary"]);
  });

  it("refreshes the backend and then bumps the filters", async () => {
    const { fixture, el, invoke } = render();
    const filters = TestBed.inject(FiltersService);
    const before = filters.generation();
    const button = [...el.querySelectorAll<HTMLButtonElement>(".layout-topbar button")].find(
      (b) => b.textContent?.trim() === "Refresh",
    )!;
    button.click();
    await fixture.whenStable();
    expect(invoke).toHaveBeenCalledWith("refresh");
    expect(filters.generation()).toBe(before + 1);
  });

  it("zooms with Ctrl + and Ctrl 0 on the document", () => {
    localStorage.clear();
    render();
    const zoom = TestBed.inject(ZoomService);
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "+", ctrlKey: true }));
    expect(zoom.factor()).toBe(1.1);
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "0", ctrlKey: true }));
    expect(zoom.factor()).toBe(1);
  });

  it("shows the version and the controls in the status bar", async () => {
    const { fixture, el } = render();
    await fixture.whenStable();
    fixture.detectChanges();
    const bar = el.querySelector("helix-status-bar")!;
    expect((bar.textContent ?? "").replace(/\s+/g, " ")).toContain("Version:v0.1.0");
    expect(bar.querySelector('app-status-bar button[aria-label="Larger"]')).not.toBeNull();
  });

  it("shows the progress bar only while loading", () => {
    const { fixture, el } = render();
    expect(el.querySelector('[role="progressbar"]')).toBeNull();
    TestBed.inject(LoadingService).set(Symbol("t"), true);
    fixture.detectChanges();
    expect(el.querySelector('[role="progressbar"]')).not.toBeNull();
  });
});

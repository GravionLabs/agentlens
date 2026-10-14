// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { provideRouter } from "@angular/router";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./app";
import { NAV, routes } from "./app.routes";
import { FiltersService } from "./core/filters.service";
import { Ipc } from "./core/ipc";
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
    providers: [provideRouter([]), { provide: Ipc, useValue: { invoke } }],
  });
  const fixture = TestBed.createComponent(App);
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement, invoke };
}

describe("app", () => {
  it("has a router configuration", () => {
    expect(Array.isArray(routes)).toBe(true);
  });

  it("links every view in the navigation, in order", () => {
    const { el } = render();
    const links = [...el.querySelectorAll<HTMLAnchorElement>('nav[aria-label="Views"] a')];
    expect(links.map((a) => a.textContent?.trim())).toEqual(NAV.map((n) => n.label));
    expect(links[0].getAttribute("href")).toBe("/overview");
  });

  it("refreshes the backend and then bumps the filters", async () => {
    const { fixture, el, invoke } = render();
    const filters = TestBed.inject(FiltersService);
    const before = filters.generation();
    const button = [...el.querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent?.trim() === "Refresh",
    )!;
    expect(button.classList.contains("hx-button")).toBe(true);
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

  it("renders the status bar below the views", () => {
    const { el } = render();
    expect(el.querySelector("app-status-bar helix-status-bar")).not.toBeNull();
  });
});

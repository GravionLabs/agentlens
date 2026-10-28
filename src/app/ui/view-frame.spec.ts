// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { provideRouter } from "@angular/router";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { LoadingService } from "../core/loading.service";
import { ViewFrame } from "./view-frame";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => {
  TestBed.resetTestingModule();
  document.querySelectorAll(".cdk-overlay-container").forEach((e) => e.remove());
});

describe("ViewFrame", () => {
  it("has the filter toolbar above the view and marks the view busy while loading", () => {
    TestBed.configureTestingModule({ providers: [provideRouter([])] });
    const fixture = TestBed.createComponent(ViewFrame);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const main = el.querySelector("main")!;
    expect(el.querySelector('section[aria-label="Filter toolbar"] app-filter-bar')).not.toBeNull();
    expect(main.getAttribute("aria-busy")).toBe("false");
    expect(main.classList.contains("busy")).toBe(false);
    TestBed.inject(LoadingService).set(Symbol("t"), true);
    fixture.detectChanges();
    expect(main.getAttribute("aria-busy")).toBe("true");
    expect(main.classList.contains("busy")).toBe(true);
  });
});

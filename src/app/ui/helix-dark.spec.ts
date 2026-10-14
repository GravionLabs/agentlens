// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { Component } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { HelixDark } from "./helix-dark";

@Component({ selector: "app-host", imports: [HelixDark], template: `<div appHelixDark></div>` })
class Host {}

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});

describe("HelixDark", () => {
  let systemDark: boolean;
  let listeners: Array<() => void>;
  let root: HTMLElement;

  beforeEach(() => {
    root = document.documentElement;
    systemDark = false;
    listeners = [];
    vi.stubGlobal("matchMedia", (query: string) => ({
      get matches() {
        return systemDark;
      },
      media: query,
      addEventListener: (_: string, l: () => void) => listeners.push(l),
      removeEventListener: (_: string, l: () => void) => {
        listeners = listeners.filter((x) => x !== l);
      },
    }));
    root.removeAttribute("data-theme");
    root.classList.remove("app-dark");
  });

  afterEach(() => {
    TestBed.resetTestingModule();
    vi.unstubAllGlobals();
    root.removeAttribute("data-theme");
    root.classList.remove("app-dark");
  });

  function mount() {
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    return fixture;
  }
  const flush = () => new Promise((r) => setTimeout(r));

  it("is dark when the system is dark", () => {
    systemDark = true;
    mount();
    expect(root.classList.contains("app-dark")).toBe(true);
  });

  it("is light when the system is light", () => {
    mount();
    expect(root.classList.contains("app-dark")).toBe(false);
  });

  it("data-theme=light overrides a dark system", () => {
    systemDark = true;
    root.setAttribute("data-theme", "light");
    mount();
    expect(root.classList.contains("app-dark")).toBe(false);
  });

  it("data-theme=dark overrides a light system", () => {
    root.setAttribute("data-theme", "dark");
    mount();
    expect(root.classList.contains("app-dark")).toBe(true);
  });

  it("follows system changes", () => {
    mount();
    systemDark = true;
    listeners.forEach((l) => l());
    expect(root.classList.contains("app-dark")).toBe(true);
  });

  it("follows data-theme changes", async () => {
    systemDark = true;
    mount();
    root.setAttribute("data-theme", "light");
    await flush();
    expect(root.classList.contains("app-dark")).toBe(false);
    root.removeAttribute("data-theme");
    await flush();
    expect(root.classList.contains("app-dark")).toBe(true);
  });

  it("stops listening when destroyed", async () => {
    const fixture = mount();
    fixture.destroy();
    root.setAttribute("data-theme", "dark");
    await flush();
    expect(listeners).toHaveLength(0);
    expect(root.classList.contains("app-dark")).toBe(false);
  });
});

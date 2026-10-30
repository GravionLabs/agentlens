// @vitest-environment jsdom
import { Component, Input, NO_ERRORS_SCHEMA } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc } from "../../core/ipc";
import samples from "../../core/samples/overview.json";
import { Overview } from "./overview";

/** Stands in for the real header: signal inputs do not bind in the JIT test setup. */
@Component({ selector: "app-page-header", template: "<h2>{{ title }}</h2><ng-content />" })
class HeaderStub {
  @Input() title = "";
}

let payload: unknown = samples;

class FakeIpc extends Ipc {
  override async invoke<T>(): Promise<T> {
    return payload as T;
  }
}

async function render() {
  TestBed.overrideComponent(Overview, {
    set: { imports: [HeaderStub], schemas: [NO_ERRORS_SCHEMA] },
  });
  TestBed.configureTestingModule({
    providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
  });
  const fixture = TestBed.createComponent(Overview);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => {
  payload = samples;
  TestBed.resetTestingModule();
  document.querySelectorAll(".cdk-overlay-container").forEach((e) => e.remove());
});

describe("Overview", () => {
  it("shows the page header and one card per source", async () => {
    const el = await render();
    expect(el.querySelector("h2")?.textContent?.trim()).toBe("Sources");
    const names = [...el.querySelectorAll(".card h3")].map((h) => h.textContent?.trim());
    expect(names).toEqual(["Claude Code", "OpenCode", "Copilot CLI", "VS Code Copilot"]);
  });

  it("shows sessions, events, warnings and roots of a found source", async () => {
    const el = await render();
    const card = el.querySelectorAll<HTMLElement>(".card")[2];
    const text = card.textContent!.replace(/\s+/g, " ");
    expect(text).toContain("1 sessions");
    expect(text).toContain("13 events");
    expect(text).toContain("2 warnings");
    expect(text).toContain("/home/user/.copilot/session-state");
    expect(card.classList.contains("missing")).toBe(false);
  });

  it("dims a missing source and says where AgentLens looked", async () => {
    const el = await render();
    const card = el.querySelectorAll<HTMLElement>(".card")[1];
    expect(card.classList.contains("missing")).toBe(true);
    expect(card.textContent).toContain("Not found.");
    expect(card.textContent).toContain("opencode.db");
  });

  it("shows no cards when there are no sources", async () => {
    payload = { ...samples, data: [] };
    const el = await render();
    expect(el.querySelectorAll(".card")).toHaveLength(0);
    expect(el.querySelector("h2")?.textContent?.trim()).toBe("Sources");
  });
});

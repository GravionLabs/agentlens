// @vitest-environment jsdom
import { Component, Input, NO_ERRORS_SCHEMA } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
import { HxField, HxSelect } from "@gravionlabs/helix-ui";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc } from "../../core/ipc";
import sessions from "../../core/samples/sessions.json";
import timeline from "../../core/samples/timeline.json";
import { Timeline } from "./timeline";

/** Stands in for the real header: signal inputs do not bind in the JIT test setup. */
@Component({ selector: "app-page-header", template: "<h2>{{ title }}</h2><ng-content />" })
class HeaderStub {
  @Input() title = "";
}

let list: unknown = sessions;
const requested: string[] = [];

class FakeIpc extends Ipc {
  override async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    if (command === "timeline")
      requested.push(`${String(args?.["source"])}|${String(args?.["id"])}`);
    return (command === "sessions" ? list : timeline) as T;
  }
}

async function render() {
  TestBed.overrideComponent(Timeline, {
    set: { imports: [HeaderStub, HxField, HxSelect], schemas: [NO_ERRORS_SCHEMA] },
  });
  TestBed.configureTestingModule({
    providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
  });
  const fixture = TestBed.createComponent(Timeline);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement };
}

async function openPicker({ fixture, el }: Awaited<ReturnType<typeof render>>) {
  el.querySelector<HTMLElement>('hx-select [role="combobox"]')!.click();
  fixture.detectChanges();
  await fixture.whenStable();
  return [...document.querySelectorAll<HTMLElement>('[role="option"]')];
}

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => {
  list = sessions;
  requested.length = 0;
  TestBed.resetTestingModule();
  document.querySelectorAll(".cdk-overlay-container").forEach((e) => e.remove());
});

describe("Timeline", () => {
  it("shows the header, the session picker and the session facts", async () => {
    const view = await render();
    const { el } = view;
    expect(el.querySelector("h2")?.textContent?.trim()).toBe("Timeline");
    expect(el.querySelector("hx-field label")!.textContent!.trim()).toBe("Session");
    const options = (await openPicker(view)).map((o) => o.textContent!.trim());
    expect(options).toHaveLength(2);
    expect(options[0]).toContain("Claude Code");
    expect(options[0]).toContain("work/demo");
    expect(el.querySelector(".meta")!.textContent!.replace(/\s+/g, " ")).toContain(
      "Claude Code · claude-test-1 · 3 requests · peak 1,060 tokens",
    );
  });

  it("loads the timeline of the session chosen in the select", async () => {
    const view = await render();
    const second = (sessions.data as { source: string; id: string }[])[1];
    const options = await openPicker(view);
    const label = options[1].textContent!.trim();
    options[1].click();
    view.fixture.detectChanges();
    await view.fixture.whenStable();
    view.fixture.detectChanges();
    expect(requested.at(-1)).toBe(`${second.source}|${second.id}`);
    expect(view.el.querySelector('hx-select [role="combobox"]')!.textContent).toContain(label);
  });

  it("draws the chart with a point per request and a marker for the compaction", async () => {
    const { el } = await render();
    const svg = el.querySelector("svg")!;
    expect(svg.getAttribute("aria-label")).toBe("Context size per request");
    expect(svg.querySelectorAll("circle")).toHaveLength(3);
    expect(svg.querySelectorAll("line.marker")).toHaveLength(1);
    expect(el.querySelectorAll(".events li")).toHaveLength(1);
    expect(el.querySelector(".events")!.textContent).toContain("before request 3");
  });

  it("says so when there are no sessions", async () => {
    list = { ...sessions, data: [] };
    const { el } = await render();
    expect(el.textContent).toContain("No sessions with requests for the selected filters.");
    expect(el.querySelector("svg")).toBeNull();
  });
});

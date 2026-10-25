// @vitest-environment jsdom
import { Component, Input, NO_ERRORS_SCHEMA } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
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

class FakeIpc extends Ipc {
  override async invoke<T>(command: string): Promise<T> {
    return (command === "sessions" ? list : timeline) as T;
  }
}

async function render() {
  TestBed.overrideComponent(Timeline, {
    set: { imports: [HeaderStub], schemas: [NO_ERRORS_SCHEMA] },
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
  return fixture.nativeElement as HTMLElement;
}

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => {
  list = sessions;
  TestBed.resetTestingModule();
  document.querySelectorAll(".cdk-overlay-container").forEach((e) => e.remove());
});

describe("Timeline", () => {
  it("shows the header, the session picker and the session facts", async () => {
    const el = await render();
    expect(el.querySelector("h2")?.textContent?.trim()).toBe("Timeline");
    const options = [...el.querySelectorAll("select option")].map((o) => o.textContent!.trim());
    expect(options).toHaveLength(2);
    expect(options[0]).toContain("Claude Code");
    expect(options[0]).toContain("work/demo");
    expect(el.querySelector(".meta")!.textContent!.replace(/\s+/g, " ")).toContain(
      "Claude Code · claude-test-1 · 3 requests · peak 1,060 tokens",
    );
  });

  it("draws the chart with a point per request and a marker for the compaction", async () => {
    const el = await render();
    const svg = el.querySelector("svg")!;
    expect(svg.getAttribute("aria-label")).toBe("Context size per request");
    expect(svg.querySelectorAll("circle")).toHaveLength(3);
    expect(svg.querySelectorAll("line.marker")).toHaveLength(1);
    expect(el.querySelectorAll(".events li")).toHaveLength(1);
    expect(el.querySelector(".events")!.textContent).toContain("before request 3");
  });

  it("says so when there are no sessions", async () => {
    list = { ...sessions, data: [] };
    const el = await render();
    expect(el.textContent).toContain("No sessions with requests for the selected filters.");
    expect(el.querySelector("svg")).toBeNull();
  });
});

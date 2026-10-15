// @vitest-environment jsdom
import { NO_ERRORS_SCHEMA } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
import { HxCell, HxColumnHeader, HxDataTable } from "@gravionlabs/helix-ui";
import { HelpLinkStub } from "../../ui/help-link.testing";
import { Ipc } from "../../core/ipc";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { tokenFormat } from "../../core/token-format.service";
import { errorRate, errorView, Flow } from "./flow";

describe("flow", () => {
  it("computes the error rate from calls, leaving unknown outcomes out of the errors", () => {
    const base = { source: "claude-code", tool: "Bash", unknown: 0, loops: 0 } as const;
    expect(errorRate({ ...base, calls: 4, errors: 1 })).toBe(0.25);
    expect(errorRate({ ...base, calls: 0, errors: 0 })).toBeNull();
  });
});

let payload: unknown = null;

class FakeIpc extends Ipc {
  override async invoke<T>(): Promise<T> {
    return { schema_version: 2, data: payload, warnings: 0, errors: [] } as T;
  }
}

async function render() {
  TestBed.overrideComponent(Flow, {
    set: {
      imports: [HxDataTable, HxCell, HxColumnHeader, HelpLinkStub],
      schemas: [NO_ERRORS_SCHEMA],
    },
  });
  const fixture = TestBed.createComponent(Flow);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement };
}

const column = (el: HTMLElement, index: number) =>
  [...el.querySelectorAll("hx-data-table tbody tr")].map((r) =>
    r.querySelectorAll("td")[index].textContent!.trim(),
  );

const typeFilter = (fixture: { detectChanges(): void }, el: HTMLElement, text: string) => {
  const input = el.querySelector<HTMLInputElement>("hx-data-table thead input")!;
  input.value = text;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  fixture.detectChanges();
};

const headerButton = (el: HTMLElement, name: string) =>
  [...el.querySelectorAll<HTMLElement>("hx-data-table th button")].find((b) =>
    b.textContent!.includes(name),
  )!;

describe("Flow tables on helix", () => {
  const growth = (session: string, peak: number, compactions: number[], near: boolean) => ({
    source: "claude-code",
    session,
    project: "/home/me/work/demo",
    model: null,
    requests: 3,
    first_tokens: 10,
    last_tokens: 20,
    peak_tokens: peak,
    compactions,
    window: null,
    window_basis: null,
    near_limit: near,
  });
  const tool = (name: string, calls: number, errors: number) => ({
    source: "opencode",
    tool: name,
    calls,
    errors,
    unknown: 0,
    loops: 0,
  });

  beforeAll(() => {
    TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
  });
  beforeEach(() => {
    tokenFormat.set("full");
    payload = {
      growth: {
        sessions: [growth("aaaaaaaa-1", 100, [], false), growth("bbbbbbbb-2", 900, [50, 60], true)],
      },
      errors: { tools: [tool("Bash", 4, 1), tool("Read", 0, 0), tool("Edit", 10, 5)], loops: [] },
      hooks: { hooks: [], permissions: [], unavailable: {} },
    };
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
  });
  afterEach(() => TestBed.resetTestingModule());

  it("shows the growth rows sorted by peak with short ids, n/a-free text and flags", async () => {
    const { el } = await render();
    // Source, Session, Project, Model, Requests, First, Last, Peak, Compactions, Near limit
    expect(column(el, 1)).toEqual(["bbbbbbbb", "aaaaaaaa"]);
    expect(column(el, 2)).toEqual(["work/demo", "work/demo"]);
    expect(column(el, 3)).toEqual(["-", "-"]);
    expect(column(el, 7)).toEqual(["900", "100"]);
    expect(column(el, 8)).toEqual(["2", "-"]);
    expect(column(el, 9)).toEqual(["yes", ""]);
    expect(el.querySelector("th[aria-sort='descending']")!.textContent).toContain("Peak");
    const tips = [...el.querySelectorAll("hx-data-table tbody span[title]")].map((s) =>
      s.getAttribute("title"),
    );
    expect(tips).toContain("Size before each compaction: 50, 60");
  });

  it("sorts growth by a header click", async () => {
    const { fixture, el } = await render();
    headerButton(el, "Peak").click();
    fixture.detectChanges();
    expect(column(el, 1)).toEqual(["aaaaaaaa", "bbbbbbbb"]);
  });

  it("shows the error rate on the tools tab, n/a when there were no calls", async () => {
    const { fixture, el } = await render();
    [...el.querySelectorAll<HTMLElement>(".tabs button")][1].click();
    fixture.detectChanges();
    // Source, Tool, Calls, Errors, Error rate, Unknown, Retry loops
    expect(column(el, 1)).toEqual(["Edit", "Bash", "Read"]);
    expect(column(el, 4)).toEqual(["50.0%", "25.0%", "n/a"]);
  });

  it("keeps the n/a error rate last in both sort directions", async () => {
    const { fixture, el } = await render();
    [...el.querySelectorAll<HTMLElement>(".tabs button")][1].click();
    fixture.detectChanges();
    headerButton(el, "Error rate").click();
    fixture.detectChanges();
    expect(column(el, 4)).toEqual(["25.0%", "50.0%", "n/a"]);
    headerButton(el, "Error rate").click();
    fixture.detectChanges();
    expect(el.querySelector("th[aria-sort='descending']")!.textContent).toContain("Error rate");
    expect(column(el, 4)).toEqual(["50.0%", "25.0%", "n/a"]);
  });

  it("filters the growth source column by the label shown, not the id", async () => {
    const { fixture, el } = await render();
    typeFilter(fixture, el, "Claude Code");
    expect(column(el, 1)).toEqual(["bbbbbbbb", "aaaaaaaa"]);
    typeFilter(fixture, el, "claude-code");
    expect(el.querySelector("hx-data-table tbody")!.textContent).toContain(
      "No sessions with requests.",
    );
  });

  it("filters the tool errors source column by the label shown", async () => {
    const { fixture, el } = await render();
    [...el.querySelectorAll<HTMLElement>(".tabs button")][1].click();
    fixture.detectChanges();
    typeFilter(fixture, el, "OpenCode");
    expect(column(el, 1)).toEqual(["Edit", "Bash", "Read"]);
    typeFilter(fixture, el, "Copilot");
    expect(el.querySelector("hx-data-table tbody")!.textContent).toContain("No tool calls.");
  });

  it("derives the sortable rate without counting unknown outcomes", () => {
    expect(errorView(tool("Bash", 4, 1)).rate).toBe(0.25);
    expect(errorView(tool("Bash", 0, 0)).rate).toBeNull();
  });

  it("says so when a table is empty", async () => {
    payload = { ...(payload as object), growth: { sessions: [] } };
    const { el } = await render();
    expect(el.querySelector("hx-data-table tbody")!.textContent).toContain(
      "No sessions with requests.",
    );
  });

  it("links the glossary terms in the growth headers", async () => {
    const { el } = await render();
    const hrefs = [...el.querySelectorAll<HTMLAnchorElement>("hx-data-table th a.help")].map((a) =>
      a.getAttribute("href"),
    );
    expect(hrefs).toEqual([
      "/glossary?term=peak",
      "/glossary?term=compaction",
      "/glossary?term=near-limit",
    ]);
  });
});

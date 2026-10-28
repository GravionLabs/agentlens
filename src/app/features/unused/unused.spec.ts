// @vitest-environment jsdom
import { NO_ERRORS_SCHEMA } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
import { HxCell, HxColumnHeader, HxDataTable } from "@gravionlabs/helix-ui";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { Ipc } from "../../core/ipc";
import { tokenFormat } from "../../core/token-format.service";
import { HelpLinkStub } from "../../ui/help-link.testing";
import type { UnusedItem } from "../../core/types";
import { referencedBy, Unused, unusedView } from "./unused";

let payload: unknown = null;

class FakeIpc extends Ipc {
  override async invoke<T>(): Promise<T> {
    return { schema_version: 2, data: payload, warnings: 0, errors: [] } as T;
  }
}

async function render() {
  TestBed.overrideComponent(Unused, {
    set: {
      imports: [HxDataTable, HxCell, HxColumnHeader, HelpLinkStub],
      schemas: [NO_ERRORS_SCHEMA],
    },
  });
  const fixture = TestBed.createComponent(Unused);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement };
}

const column = (el: HTMLElement, index: number, table = 0) =>
  [...el.querySelectorAll("hx-data-table")[table].querySelectorAll("tbody tr")].map((r) =>
    r.querySelectorAll("td")[index].textContent!.trim(),
  );

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
beforeEach(() => {
  localStorage.clear();
  tokenFormat.set("full");
  TestBed.configureTestingModule({
    providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
  });
});
afterEach(() => TestBed.resetTestingModule());

const item = (over: Partial<UnusedItem>): UnusedItem => ({
  kind: "skill",
  name: "verify-project",
  project: null,
  clients: ["claude-code"],
  uses: 0,
  requests: 10,
  overhead_tokens: { count: 100, measured: false },
  ...over,
});

describe("referencedBy", () => {
  it("names the agents, commands and skills that mention the skill", () => {
    expect(
      referencedBy(
        item({
          referenced_by: [
            { kind: "agent", name: "dev-workflow" },
            { kind: "command", name: "ship" },
          ],
        }),
      ),
    ).toBe("agent dev-workflow, command ship");
  });

  it("is empty when nothing names the skill", () => {
    expect(referencedBy(item({}))).toBe("");
    expect(referencedBy(item({ referenced_by: [] }))).toBe("");
  });
});

describe("unusedView", () => {
  it("marks computed idle cost as an estimate and unknown cost as null", () => {
    expect(unusedView(item({}))).toMatchObject({ cost: 100, costText: "~100", estimated: true });
    expect(unusedView(item({ overhead_tokens: { count: 7, measured: true } }))).toMatchObject({
      costText: "7",
      estimated: false,
    });
    expect(unusedView(item({ overhead_tokens: null }))).toMatchObject({
      cost: null,
      costText: "unknown",
      estimated: false,
    });
  });

  it("joins the client labels", () => {
    expect(unusedView(item({ clients: ["claude-code", "opencode"] })).clients).toBe(
      "Claude Code, OpenCode",
    );
  });
});

describe("Unused table on helix", () => {
  beforeEach(() => {
    payload = {
      items: [
        item({ name: "cheap", overhead_tokens: { count: 5, measured: true } }),
        item({ name: "mcp", kind: "mcp", overhead_tokens: null, project: "/work/team/demo" }),
        item({ name: "dear", overhead_tokens: { count: 900, measured: false } }),
        item({ name: "used", uses: 3 }),
      ],
    };
  });

  it("shows unused items by idle cost, with unknown last and a tilde on estimates", async () => {
    const { el } = await render();
    expect(column(el, 1)).toEqual(["dear", "cheap", "mcp"]);
    expect(column(el, 6)).toEqual(["~900", "5", "unknown"]);
    expect(column(el, 2)[2]).toBe("team/demo");
  });

  it("can include the items that were used, keeps the glossary links and filters by kind", async () => {
    const { fixture, el } = await render();
    expect(
      [...el.querySelectorAll("hx-data-table th a.help")].map((a) => a.getAttribute("aria-label")),
    ).toEqual(["Explain Requests", "Explain Idle cost"]);
    el.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
    fixture.detectChanges();
    expect(column(el, 1)).toHaveLength(4);
    const filter = el.querySelector<HTMLInputElement>(
      "hx-data-table .hx-data-table-filter-row input",
    )!;
    filter.value = "mcp";
    filter.dispatchEvent(new Event("input"));
    fixture.detectChanges();
    expect(column(el, 1)).toEqual(["mcp"]);
  });

  it("says so when everything was used", async () => {
    payload = { items: [item({ uses: 2 })] };
    const { el } = await render();
    expect(el.querySelector("hx-data-table tbody")!.textContent).toContain(
      "Everything installed was used.",
    );
  });
});

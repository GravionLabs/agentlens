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
import type { ContextKind } from "../../core/types";
import type { ContextRow } from "../../core/types";
import { byOwner, Context, firstRequestNote, inView } from "./context";

describe("context views", () => {
  const kinds: ContextKind[] = [
    "tool-listing",
    "tool-definition",
    "mcp-instructions",
    "agent-listing",
    "skill-listing",
    "system-prompt",
    "instruction-file",
  ];

  it("groups the kinds like `agentlens context --kind`", () => {
    expect(kinds.filter((k) => inView(k, "tools"))).toEqual([
      "tool-listing",
      "tool-definition",
      "mcp-instructions",
    ]);
    expect(kinds.filter((k) => inView(k, "instructions"))).toEqual(["instruction-file"]);
    expect(kinds.filter((k) => inView(k, "agents"))).toEqual(["agent-listing"]);
    expect(kinds.filter((k) => inView(k, "skills"))).toEqual(["skill-listing"]);
    expect(kinds.filter((k) => inView(k, "all"))).toEqual(kinds);
  });
});

describe("context grouping", () => {
  const row = (project: string, tokens: number, requests: number): ContextRow => ({
    source: "claude-code",
    project,
    kind: "mcp-instructions",
    owner: "srv",
    items: 1,
    bytes: 10,
    tokens: { count: tokens, measured: false },
    requests,
    overhead_tokens: { count: tokens * requests, measured: false },
  });

  it("sums the projects of one owner into a single row", () => {
    const rows = byOwner([
      row("/a", 10, 2),
      row("/b", 5, 4),
      { ...row("/a", 1, 1), owner: "other" },
    ]);
    expect(rows).toHaveLength(2);
    const srv = rows.find((r) => r.owner === "srv")!;
    expect(srv).toMatchObject({
      items: 2,
      bytes: 20,
      tokens: { count: 15, measured: false },
      requests: 6,
      overhead_tokens: { count: 40, measured: false },
    });
    expect(srv.project).toBeNull();
  });

  it("keeps a sum an estimate unless every part was measured", () => {
    const measured = { ...row("/a", 10, 2), tokens: { count: 10, measured: true } };
    const both = byOwner([measured, { ...measured, project: "/b" }]);
    expect(both[0].tokens).toEqual({ count: 20, measured: true });
    const mixed = byOwner([measured, row("/b", 5, 4)]);
    expect(mixed[0].tokens).toEqual({ count: 15, measured: false });
  });
});

describe("first request note", () => {
  const f = { sessions: 2, total_tokens: 4_228_610, max_tokens: 2_500_000 };
  afterEach(() => tokenFormat.set("full"));

  it("shows full counts without a title by default", () => {
    const n = firstRequestNote("claude-code", f);
    expect(n.text).toContain("holds 2,114,305 tokens on average, at most 2,500,000 (");
    expect(n.title).toBeNull();
  });

  it("rounds the counts and keeps the exact ones in the title", () => {
    tokenFormat.set("rounded");
    const n = firstRequestNote("claude-code", f);
    expect(n.text).toContain("holds 2.11 M tokens on average, at most 2.50 M (");
    expect(n.title).toBe("Average 2,114,305 tokens, at most 2,500,000");
  });
});

let payload: unknown = null;

class FakeIpc extends Ipc {
  override async invoke<T>(): Promise<T> {
    return { schema_version: 2, data: payload, warnings: 0, errors: [] } as T;
  }
}

async function render() {
  TestBed.overrideComponent(Context, {
    set: {
      imports: [HxDataTable, HxCell, HxColumnHeader, HelpLinkStub],
      schemas: [NO_ERRORS_SCHEMA],
    },
  });
  const fixture = TestBed.createComponent(Context);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement };
}

const column = (el: HTMLElement, index: number) =>
  [...el.querySelectorAll("hx-data-table tbody tr")].map((r) =>
    r.querySelectorAll("td")[index].textContent!.trim(),
  );

const headerButton = (el: HTMLElement, name: string) =>
  [...el.querySelectorAll<HTMLElement>("hx-data-table th button")].find((b) =>
    b.textContent!.includes(name),
  )!;

describe("Context table on helix", () => {
  const item = (owner: string, tokens: number, measured: boolean, requests: number) => ({
    source: "claude-code",
    project: "/work/demo",
    kind: "mcp-instructions",
    owner,
    items: 1,
    bytes: 10,
    tokens: { count: tokens, measured },
    requests,
    overhead_tokens: { count: tokens * requests, measured },
  });

  beforeAll(() => {
    TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
  });
  beforeEach(() => {
    tokenFormat.set("full");
    payload = {
      rows: [item("small", 10, true, 2), item("big", 50, false, 4), item("mid", 20, true, 5)],
      first_request: {},
      unavailable: {},
    };
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
  });
  afterEach(() => TestBed.resetTestingModule());

  it("sorts by overhead, descending, and marks estimated counts", async () => {
    const { el } = await render();
    // Columns: Source, Kind, Owner, Items, Tokens, Requests, Overhead
    expect(column(el, 2)).toEqual(["big", "mid", "small"]);
    expect(column(el, 6)).toEqual(["~200", "100", "20"]);
    expect(column(el, 4)).toEqual(["~50", "20", "10"]);
    expect(el.querySelectorAll("hx-data-table tbody span.est")).toHaveLength(2);
    expect(el.querySelector("th[aria-sort='descending']")!.textContent).toContain("Overhead");
  });

  it("sorts by a header click: ascending first", async () => {
    const { fixture, el } = await render();
    headerButton(el, "Requests").click();
    fixture.detectChanges();
    expect(column(el, 2)).toEqual(["small", "big", "mid"]);
  });

  it("shows headers, a filter on the source column and a tooltip with the owner path", async () => {
    const { el } = await render();
    const head = el.querySelector("hx-data-table thead")!.textContent!;
    for (const h of ["Source", "Kind", "Owner", "Items", "Tokens", "Requests", "Overhead"]) {
      expect(head).toContain(h);
    }
    expect(el.querySelector("hx-data-table thead input")).not.toBeNull();
    expect(column(el, 0)[0]).toBe("Claude Code");
  });

  it("says so when there is nothing to show", async () => {
    payload = { rows: [], first_request: {}, unavailable: {} };
    const { el } = await render();
    expect(el.querySelector("hx-data-table tbody")!.textContent).toContain(
      "No context items found.",
    );
  });

  it("links the glossary terms in the headers and offers all rows beyond the first 100", async () => {
    const { el } = await render();
    const hrefs = [...el.querySelectorAll<HTMLAnchorElement>("hx-data-table th a.help")].map((a) =>
      a.getAttribute("href"),
    );
    expect(hrefs).toEqual([
      "/glossary?term=kind",
      "/glossary?term=owner",
      "/glossary?term=token",
      "/glossary?term=request",
      "/glossary?term=overhead",
    ]);
    expect(el.querySelector("hx-paginator")).toBeNull();
  });

  it("pages above 100 rows with an option to show all", async () => {
    payload = {
      rows: Array.from({ length: 101 }, (_, i) => item(`o${i}`, i + 1, true, 1)),
      first_request: {},
      unavailable: {},
    };
    const { el } = await render();
    expect(column(el, 2)).toHaveLength(100);
    const paginator = el.querySelector("hx-paginator")!;
    expect(paginator.textContent).toContain("1 - 100 of 101");
    expect(paginator.querySelector("hx-select")).not.toBeNull();
  });
});

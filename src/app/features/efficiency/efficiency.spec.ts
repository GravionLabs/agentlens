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
import { Efficiency, groupLabel, share, subagentView } from "./efficiency";

let payload: unknown = null;

class FakeIpc extends Ipc {
  override async invoke<T>(): Promise<T> {
    return { schema_version: 2, data: payload, warnings: 0, errors: [] } as T;
  }
}

async function render() {
  TestBed.overrideComponent(Efficiency, {
    set: {
      imports: [HxDataTable, HxCell, HxColumnHeader, HelpLinkStub],
      schemas: [NO_ERRORS_SCHEMA],
    },
  });
  const fixture = TestBed.createComponent(Efficiency);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement };
}

const column = (el: HTMLElement, index: number, table = 0) =>
  [...el.querySelectorAll("hx-data-table")[table].querySelectorAll("tbody tr")].map((r) =>
    r.querySelectorAll("td")[index].textContent!.trim(),
  );

const headerButton = (el: HTMLElement, name: string) =>
  [...el.querySelectorAll<HTMLElement>("hx-data-table th button")].find((b) =>
    b.textContent!.includes(name),
  )!;

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

describe("efficiency", () => {
  it("shares a total and has no share of nothing", () => {
    expect(share(1, 4)).toBe(0.25);
    expect(share(0, 0)).toBeNull();
  });

  it("shortens long ids and keeps the last segments of paths", () => {
    expect(groupLabel("0123456789abcdef0123456789abcdef-x")).toBe("01234567");
    expect(groupLabel("claude-code")).toBe("claude-code");
    expect(groupLabel("/work/team/demo")).not.toContain("/work/team/demo");
  });

  it("precomputes the subagent shares and keeps n/a as null", () => {
    const v = subagentView({
      source: "claude-code",
      requests: 4,
      subagent_requests: 1,
      tokens: 0,
      subagent_tokens: 0,
      usd: 10,
      subagent_usd: 5,
      unpriced_requests: 2,
    } as never);
    expect(v).toMatchObject({ subRequests: 0.25, subTokens: null, subCost: 0.5, unpriced: 2 });
  });
});

describe("Efficiency tables on helix", () => {
  const cache = (key: string, requests: number, over = {}) => ({
    key,
    source: "claude-code",
    requests,
    input: 2_114_305,
    cache_read: 10,
    cache_write: 0,
    hit_rate: 0.5,
    ...over,
  });
  beforeEach(() => {
    payload = {
      cache: [cache("a", 1), cache("b", 7, { hit_rate: null }), cache("c", 3)],
      subagents: [
        {
          source: "claude-code",
          requests: 4,
          subagent_requests: 1,
          tokens: 100,
          subagent_tokens: 50,
          usd: 0,
          subagent_usd: 0,
          unpriced_requests: 0,
        },
      ],
    };
  });

  it("sorts by requests descending first and shows n/a for a missing hit rate", async () => {
    const { el } = await render();
    expect(column(el, 0)).toEqual(["b", "c", "a"]);
    expect(column(el, 6)).toEqual(["n/a", "50.0%", "50.0%"]);
  });

  it("keeps the glossary links, the source label and the exact token title", async () => {
    tokenFormat.set("rounded");
    const { el } = await render();
    const links = [...el.querySelectorAll("hx-data-table th a.help")].map((a) =>
      a.getAttribute("aria-label"),
    );
    expect(links).toContain("Explain Hit rate");
    expect(links).toContain("Explain Unpriced requests");
    expect(column(el, 1)[0]).toBe("Claude Code");
    const input = el.querySelector("hx-data-table tbody tr td:nth-child(4) span")!;
    expect(input.textContent!.trim()).toBe("2.11 M");
    expect(input.getAttribute("title")).toBe("2,114,305");
  });

  it("shows the subagent shares as percentages", async () => {
    const { el } = await render();
    expect(column(el, 2, 1)).toEqual(["25.0%"]);
    expect(column(el, 3, 1)).toEqual(["50.0%"]);
    expect(column(el, 4, 1)).toEqual(["n/a"]);
  });

  it("sorts a column on click", async () => {
    const { fixture, el } = await render();
    headerButton(el, "Group").click();
    fixture.detectChanges();
    expect(column(el, 0)).toEqual(["a", "b", "c"]);
  });
});

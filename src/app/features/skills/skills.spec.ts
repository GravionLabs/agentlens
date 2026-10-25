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
import type { Answer, SkillsData } from "../../core/types";
import samples from "../../core/samples/skills.json";
import { overhead, Skills, skillView } from "./skills";

let payload: unknown = null;

class FakeIpc extends Ipc {
  override async invoke<T>(): Promise<T> {
    return { schema_version: 2, data: payload, warnings: 0, errors: [] } as T;
  }
}

async function render() {
  TestBed.overrideComponent(Skills, {
    set: {
      imports: [HxDataTable, HxCell, HxColumnHeader, HelpLinkStub],
      schemas: [NO_ERRORS_SCHEMA],
    },
  });
  const fixture = TestBed.createComponent(Skills);
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

const stats = (name: string, over: Record<string, unknown> = {}) =>
  ({
    name,
    invocations: 1,
    sessions: 1,
    file_uses: 0,
    load: { count: 100, measured: false },
    carry: { tokens: { count: 50, measured: true }, requests: 1 },
    baseline: { tokens: { count: 0, measured: false } },
    attributed: { requests: 0, input: 0, output: 0, cache_read: 0, cache_write: 0 },
    follow_on: { input: 1, output: 2, cache_read: 3, cache_write: 4 },
    last_used: null,
    ...over,
  }) as never;

describe("skill overhead", () => {
  it("adds baseline, load and carry like the CLI", () => {
    const skills = (samples as unknown as Answer<SkillsData>).data.report.skills;
    const demo = skills.find((s) => s.name === "demo")!;
    expect(overhead(demo)).toBe(
      demo.baseline.tokens.count + demo.load.count + demo.carry.tokens.count,
    );
    expect(overhead(demo)).toBe(24);
  });
});

describe("skillView", () => {
  it("marks only non-zero computed values as estimates", () => {
    const v = skillView(stats("a"));
    expect(v).toMatchObject({
      load: 100,
      loadEst: true,
      carry: 50,
      carryEst: false,
      baseline: 0,
      baselineEst: false,
      overhead: 150,
      overheadEst: true,
      follow_on: 10,
      attributed: null,
    });
  });

  it("has an attributed count only when requests were attributed", () => {
    const v = skillView(
      stats("a", {
        attributed: { requests: 2, input: 1, output: 1, cache_read: 1, cache_write: 1 },
      }),
    );
    expect(v.attributed).toBe(4);
  });
});

describe("Skills table on helix", () => {
  beforeEach(() => {
    payload = {
      installed_skills: 3,
      report: {
        skills: [
          stats("small", { load: { count: 1, measured: true } }),
          stats("big", { load: { count: 5000, measured: false } }),
          stats("idle", { invocations: 0, load: { count: 0, measured: true } }),
        ],
      },
    };
  });

  it("sorts by overhead descending and marks estimates with a tilde", async () => {
    const { el } = await render();
    expect(column(el, 0)).toEqual(["big", "idle", "small"].sort((a, b) => order(a) - order(b)));
    expect(column(el, 4)[0]).toBe("~5,000");
    expect(column(el, 4)[2]).toBe("0");
    expect(el.querySelector("hx-data-table tbody tr td:nth-child(5) span")!.className).toContain(
      "est",
    );
  });

  it("dims skills that were never used", async () => {
    const { el } = await render();
    const rows = [...el.querySelectorAll("hx-data-table tbody tr")];
    expect(rows.map((r) => r.classList.contains("unused"))).toEqual([false, false, true]);
  });

  it("keeps the glossary links, shows - for a missing last use and the used-only filter", async () => {
    const { fixture, el } = await render();
    const labels = [...el.querySelectorAll("hx-data-table th a.help")].map((a) =>
      a.getAttribute("aria-label"),
    );
    expect(labels).toEqual(
      expect.arrayContaining(["Explain Load", "Explain Overhead", "Explain Follow-on"]),
    );
    expect(column(el, 9)[0]).toBe("-");
    el.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
    fixture.detectChanges();
    expect(column(el, 0)).toHaveLength(2);
  });

  it("rounds counts with the token format and keeps the exact one as title", async () => {
    tokenFormat.set("rounded");
    const { el } = await render();
    const cell = el.querySelector("hx-data-table tbody tr td:nth-child(5) span")!;
    expect(cell.textContent!.trim()).toBe("~5.00 k");
    expect(cell.getAttribute("title")).toBe("5,000");
  });
});

const order = (n: string) => ["big", "small", "idle"].indexOf(n);

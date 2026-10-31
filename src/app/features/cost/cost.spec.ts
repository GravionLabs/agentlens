// @vitest-environment jsdom
import { NO_ERRORS_SCHEMA } from "@angular/core";
import { HxCell, HxDataTable } from "@gravionlabs/helix-ui";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
import { beforeAll, beforeEach, afterEach, describe, expect, it } from "vitest";
import { Ipc } from "../../core/ipc";
import { tokenFormat } from "../../core/token-format.service";
import type { CostRow } from "../../core/types";
import type { WithoutReport } from "../../core/types";
import { WithoutService } from "../../core/without.service";
import { Cost, money, signedUsd, whatIfView, withoutViews } from "./cost";

const row = (over: Partial<CostRow>): CostRow => ({
  key: "m",
  requests: 2,
  input: 0,
  output: 0,
  cache_read: 0,
  cache_write: 0,
  measured_usd: 0,
  estimated_usd: 0,
  unpriced_requests: 0,
  premium_requests: 0,
  ...over,
});

describe("cost money", () => {
  it("marks computed values with a tilde and mixes both like the CLI", () => {
    expect(money(row({ estimated_usd: 1.5 }))).toBe("~$1.50");
    expect(money(row({ measured_usd: 2 }))).toBe("$2.00");
    expect(money(row({ measured_usd: 2, estimated_usd: 1.5 }))).toBe("$2.00 + ~$1.50");
  });

  it("says unpriced instead of showing zero when nothing has a price", () => {
    expect(money(row({ unpriced_requests: 2 }))).toBe("unpriced");
    expect(money(row({ requests: 0 }))).toBe("$0.00");
  });
});

describe("cost what-if", () => {
  it("computes the difference and signs it", () => {
    const v = whatIfView({
      model: "m",
      requests: 1,
      actual_usd: 3,
      what_if_usd: 1,
      excluded_requests: 0,
    });
    expect(v.diff).toBe(-2);
    expect(signedUsd(v.diff)).toBe("-$2.00");
    expect(signedUsd(1.5)).toBe("+$1.50");
  });
});

let rows: CostRow[] = [];
const baseRows = [
  row({ key: "model-a", requests: 1, input: 99999, output: 9999, measured_usd: 0.5 }),
  row({ key: "model-b", requests: 7, input: 10, output: 5, estimated_usd: 2 }),
  row({ key: "model-c", requests: 3, input: 20, output: 7, unpriced_requests: 3 }),
];

let withoutReport: WithoutReport | null = null;
let lastArgs: Record<string, unknown> = {};

class FakeIpc extends Ipc {
  override async invoke<T>(_command: string, args?: Record<string, unknown>): Promise<T> {
    lastArgs = args ?? {};
    const data = {
      report: { rows, total: rows[0], unpriced_models: {} },
      what_if: null,
      without: withoutReport,
    };
    return { schema_version: 2, data, warnings: 0, errors: [] } as T;
  }
}

describe("Cost table on helix", () => {
  beforeAll(() => {
    TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
  });
  beforeEach(() => {
    localStorage.clear();
    tokenFormat.set("rounded");
    rows = baseRows;
    withoutReport = null;
    lastArgs = {};
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
  });
  afterEach(() => TestBed.resetTestingModule());

  async function render(before: (w: WithoutService) => void = () => undefined) {
    // Signal inputs of app components need the Angular compiler plugin; only the table is under test.
    TestBed.overrideComponent(Cost, {
      set: { imports: [HxDataTable, HxCell], schemas: [NO_ERRORS_SCHEMA] },
    });
    before(TestBed.inject(WithoutService));
    const fixture = TestBed.createComponent(Cost);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    return { fixture, el: fixture.nativeElement as HTMLElement };
  }

  const firstColumn = (el: HTMLElement) =>
    [...el.querySelectorAll("hx-data-table tbody tr")].map((r) =>
      r.querySelector("td")!.textContent!.trim(),
    );

  it("renders the rows sorted by cost, descending, with n/a-free text and a tilde", async () => {
    const { el } = await render();
    expect(firstColumn(el)).toEqual(["model-b", "model-a", "model-c"]);
    const text = el.querySelector("hx-data-table tbody")!.textContent!;
    expect(text).toContain("~$2.00");
    expect(text).toContain("unpriced");
    expect(el.querySelector("th[aria-sort='descending']")!.textContent).toContain("Cost");
  });

  const headerButton = (el: HTMLElement, name: string) =>
    [...el.querySelectorAll<HTMLElement>("hx-data-table th button")].find((b) =>
      b.textContent!.includes(name),
    )!;

  it("sorts by a header click: ascending first, then descending", async () => {
    const { fixture, el } = await render();
    headerButton(el, "Requests").click();
    fixture.detectChanges();
    expect(firstColumn(el)).toEqual(["model-a", "model-c", "model-b"]);
    expect(el.querySelector("th[aria-sort='ascending']")!.textContent).toContain("Requests");
    headerButton(el, "Requests").click();
    fixture.detectChanges();
    expect(firstColumn(el)).toEqual(["model-b", "model-c", "model-a"]);
    expect(el.querySelector("th[aria-sort='descending']")!.textContent).toContain("Requests");
  });

  it("shows the premium requests column only when a row has premium requests", async () => {
    const without = await render();
    expect(without.el.querySelector("hx-data-table thead")!.textContent).not.toContain("Premium");
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
    rows = [...baseRows, row({ key: "model-d", premium_requests: 4 })];
    const withPremium = await render();
    expect(withPremium.el.querySelector("hx-data-table thead")!.textContent).toContain(
      "Premium requests",
    );
  });

  it("pages above 50 rows and shows everything on one page below", async () => {
    const small = await render();
    expect(firstColumn(small.el)).toHaveLength(3);
    expect(small.el.querySelector("hx-paginator")).toBeNull();
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
    rows = Array.from({ length: 51 }, (_, i) => row({ key: `m-${i}`, measured_usd: i + 1 }));
    const large = await render();
    expect(firstColumn(large.el)).toHaveLength(50);
    expect(large.el.querySelector("hx-paginator")).not.toBeNull();
  });

  it("puts the exact token count in the tooltip while counts are rounded", async () => {
    const { el } = await render();
    const titles = [...el.querySelectorAll("hx-data-table tbody span[title]")].map((s) =>
      s.getAttribute("title"),
    );
    expect(titles).toContain("99,999");
    expect(titles).toContain("model-a");
  });

  const report: WithoutReport = {
    rows: [
      {
        item: "demo",
        kind: "skill",
        installations: 2,
        requests: 4,
        tokens: { count: 1200, measured: false },
        cost_usd: 0.5,
        unpriced_requests: 0,
        detail: "",
      },
      {
        item: "github",
        kind: "mcp-server",
        installations: 1,
        requests: 3,
        tokens: { count: 300, measured: false },
        cost_usd: 0,
        unpriced_requests: 3,
        detail: "",
      },
    ],
    total: {
      requests: 7,
      tokens: { count: 1500, measured: false },
      cost_usd: 0.5,
      unpriced_requests: 3,
    },
    notes: ["Estimated: from listing sizes."],
    warnings: ["`skill:gone` matched nothing installed"],
  };

  it("shows no chips and no without table while the list is empty", async () => {
    const { el } = await render();
    expect(el.querySelector(".chips")).toBeNull();
    expect(el.textContent).not.toContain("What if removed");
    expect(lastArgs["without"]).toEqual([]);
  });

  it("shows one removable chip per item and asks the analysis with the list", async () => {
    const { fixture, el } = await render((w) => {
      w.add("skill", "demo");
      w.add("mcp-server", "github");
    });
    const chips = [...el.querySelectorAll(".chip")].map((c) =>
      c.textContent!.replace(/\s+/g, " ").trim(),
    );
    expect(chips).toEqual(["skill:demo ×", "mcp-server:github ×"]);
    expect(lastArgs["without"]).toEqual(["skill:demo", "mcp-server:github"]);
    el.querySelector<HTMLButtonElement>('.chip button[aria-label="Remove skill:demo"]')!.click();
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(TestBed.inject(WithoutService).items()).toEqual(["mcp-server:github"]);
    expect(lastArgs["without"]).toEqual(["mcp-server:github"]);
  });

  it("shows the without table with tildes, a total row, the notes and the warnings", async () => {
    withoutReport = report;
    const { el } = await render((w) => w.add("skill", "demo"));
    const table = el.querySelector(
      'hx-data-table[ariaLabel="What if removed"], hx-data-table:last-of-type',
    )!;
    const rows = [...table.querySelectorAll("tbody tr")].map((r) =>
      [...r.querySelectorAll("td")].map((c) => c.textContent!.trim()),
    );
    expect(rows).toEqual([
      ["demo", "skill", "2", "4", "~1.20 k", "~$0.50"],
      ["github", "mcp-server", "1", "3", "~300", "n/a"],
      ["Total", "", "", "7", "~1.50 k", "~$0.50"],
    ]);
    expect(el.textContent).toContain("Estimated: from listing sizes.");
    expect(el.textContent).toContain("`skill:gone` matched nothing installed");
  });
});

describe("withoutViews", () => {
  it("puts the total last and writes n/a for a row without any price", () => {
    const views = withoutViews({
      rows: [],
      total: {
        requests: 2,
        tokens: { count: 5, measured: false },
        cost_usd: 0,
        unpriced_requests: 2,
      },
      notes: [],
      warnings: [],
    });
    expect(views).toHaveLength(1);
    expect(views[0]).toMatchObject({ item: "Total", costText: "n/a", tokens: 5 });
  });
});

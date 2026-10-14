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
import { Cost, money } from "./cost";

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

let rows: CostRow[] = [];
const baseRows = [
  row({ key: "model-a", requests: 1, input: 99999, output: 9999, measured_usd: 0.5 }),
  row({ key: "model-b", requests: 7, input: 10, output: 5, estimated_usd: 2 }),
  row({ key: "model-c", requests: 3, input: 20, output: 7, unpriced_requests: 3 }),
];

class FakeIpc extends Ipc {
  override async invoke<T>(): Promise<T> {
    const data = { report: { rows, total: rows[0], unpriced_models: {} }, what_if: null };
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
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
  });
  afterEach(() => TestBed.resetTestingModule());

  async function render() {
    // Signal inputs of app components need the Angular compiler plugin; only the table is under test.
    TestBed.overrideComponent(Cost, {
      set: { imports: [HxDataTable, HxCell], schemas: [NO_ERRORS_SCHEMA] },
    });
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
});

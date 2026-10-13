// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc } from "../core/ipc";
import { tokens } from "../core/format";
import { TokenFormatService, tokenFormat } from "../core/token-format.service";
import { type Column, DataTable } from "./data-table";
import { StatusBar } from "./status-bar";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
  // Vitest compiles components just in time, without the compiler's signal input metadata.
  const def = (DataTable as unknown as { ɵcmp: Record<string, unknown> }).ɵcmp;
  const inputs: Record<string, unknown> = {};
  for (const name of ["columns", "data"]) inputs[name] = [name, 1, null]; // flag 1: signal based
  def["inputs"] = inputs;
  def["declaredInputs"] = { columns: "columns", data: "data" };
});
afterEach(() => {
  tokenFormat.set("full");
  TestBed.resetTestingModule();
});

interface Row {
  input: number;
  requests: number;
}

const columns: Column<Row>[] = [
  {
    key: "input",
    label: "Input",
    value: (r) => r.input,
    text: (r) => tokens(r.input),
    tokens: true,
  },
  { key: "requests", label: "Requests", value: (r) => r.requests },
];

/** The title the template binds to a cell (`tip` is protected, the template is its only caller). */
function titles(row: Row) {
  const table = DataTable.prototype as unknown as { tip(c: Column<Row>, r: Row): string | null };
  return columns.map((c) => table.tip(c, row));
}

const row: Row = { input: 2_114_305, requests: 4_000 };

describe("DataTable token cells", () => {
  it("has the exact value as title when counts are rounded", () => {
    tokenFormat.set("rounded");
    expect(tokens(row.input)).toBe("2.11 M");
    expect(titles(row)).toEqual(["2,114,305", null]);
  });

  it("needs no title when counts are shown in full or are small", () => {
    expect(titles(row)).toEqual([null, null]);
    tokenFormat.set("rounded");
    expect(titles({ input: 999, requests: 1 })).toEqual([null, null]);
  });

  it("keeps an explicit hover", () => {
    tokenFormat.set("rounded");
    const col: Column<Row> = { ...columns[0], hover: () => "note" };
    const table = DataTable.prototype as unknown as { tip(c: Column<Row>, r: Row): string | null };
    expect(table.tip(col, row)).toBe("note");
  });
});

describe("DataTable rendering", () => {
  function render() {
    const fixture = TestBed.createComponent(DataTable<Row>);
    fixture.componentRef.setInput("columns", columns);
    fixture.componentRef.setInput("data", [row]);
    fixture.detectChanges();
    const cells = () =>
      Array.from((fixture.nativeElement as HTMLElement).querySelectorAll<HTMLElement>("tbody td"));
    return { fixture, cells };
  }

  it("switches the cell text and tooltip with the token format service", () => {
    const { fixture, cells } = render();
    expect(cells()[0].textContent?.trim()).toBe("2,114,305");
    expect(cells()[0].hasAttribute("title")).toBe(false);

    TestBed.inject(TokenFormatService).set("rounded");
    fixture.detectChanges();
    expect(cells()[0].textContent?.trim()).toBe("2.11 M");
    expect(cells()[0].getAttribute("title")).toBe("2,114,305");
    expect(cells()[1].textContent?.trim()).toBe("4000");
    expect(cells()[1].hasAttribute("title")).toBe(false);

    TestBed.inject(TokenFormatService).set("full");
    fixture.detectChanges();
    expect(cells()[0].textContent?.trim()).toBe("2,114,305");
    expect(cells()[0].hasAttribute("title")).toBe(false);
  });

  it("follows the status bar switch", () => {
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useValue: { invoke: async () => "0.1.0" } }],
    });
    const { fixture, cells } = render();
    const bar = TestBed.createComponent(StatusBar);
    bar.detectChanges();
    bar.nativeElement.querySelector('button[aria-label="Round token counts"]').click();
    fixture.detectChanges();
    expect(cells()[0].textContent?.trim()).toBe("2.11 M");
    expect(cells()[0].getAttribute("title")).toBe("2,114,305");
  });
});

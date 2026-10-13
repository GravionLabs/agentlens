import { afterEach, describe, expect, it } from "vitest";
import { tokens } from "../core/format";
import { tokenFormat } from "../core/token-format.service";
import { type Column, DataTable } from "./data-table";

afterEach(() => tokenFormat.set("full"));

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

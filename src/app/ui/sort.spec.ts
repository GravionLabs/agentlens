import { describe, expect, it } from "vitest";
import { nullsLast } from "./sort";

const rows = [{ v: 2 }, { v: null }, { v: 5 }, { v: 1 }];
const values = (r: readonly unknown[]) => r.map((x) => (x as { v: number | null }).v);

describe("nullsLast", () => {
  it("sorts ascending with missing values at the end", () => {
    expect(values(nullsLast(rows, [{ field: "v", order: 1 }]))).toEqual([1, 2, 5, null]);
  });

  it("sorts descending with missing values still at the end", () => {
    expect(values(nullsLast(rows, [{ field: "v", order: -1 }]))).toEqual([5, 2, 1, null]);
  });

  it("does not change the input", () => {
    nullsLast(rows, [{ field: "v", order: 1 }]);
    expect(values(rows)).toEqual([2, null, 5, 1]);
  });
});

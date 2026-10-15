import { compareValues, resolveField, type HxSortFunction } from "@gravionlabs/helix-ui";

/**
 * A table sort that keeps missing values (`null`, n/a) at the end in both directions, as the
 * hand-written table did; helix puts them first when sorting descending.
 */
export const nullsLast: HxSortFunction = (rows, meta) =>
  [...rows].sort((a, b) => {
    for (const { field, order } of meta) {
      const x = resolveField(a, field);
      const y = resolveField(b, field);
      const xEmpty = x === null || x === undefined || x === "";
      const yEmpty = y === null || y === undefined || y === "";
      if (xEmpty || yEmpty) {
        if (xEmpty && yEmpty) continue;
        return xEmpty ? 1 : -1;
      }
      const result = compareValues(x, y);
      if (result) return result * order;
    }
    return 0;
  });

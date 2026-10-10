import { describe, expect, it } from "vitest";
import type { CostRow } from "../../core/types";
import { money } from "./cost";

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

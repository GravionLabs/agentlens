import { describe, expect, it } from "vitest";
import { errorRate } from "./flow";

describe("flow", () => {
  it("computes the error rate from calls, leaving unknown outcomes out of the errors", () => {
    const base = { source: "claude-code", tool: "Bash", unknown: 0, loops: 0 } as const;
    expect(errorRate({ ...base, calls: 4, errors: 1 })).toBe(0.25);
    expect(errorRate({ ...base, calls: 0, errors: 0 })).toBeNull();
  });
});

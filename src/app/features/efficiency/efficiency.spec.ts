import { describe, expect, it } from "vitest";
import { share } from "./efficiency";

describe("efficiency", () => {
  it("shares a total and has no share of nothing", () => {
    expect(share(1, 4)).toBe(0.25);
    expect(share(0, 0)).toBeNull();
  });
});

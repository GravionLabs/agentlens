import { describe, expect, it } from "vitest";
import { LoadingService } from "./loading.service";

describe("LoadingService", () => {
  it("is busy while any analysis loads", () => {
    const s = new LoadingService();
    const a = Symbol("a");
    const b = Symbol("b");
    expect(s.busy()).toBe(false);
    s.set(a, true);
    s.set(b, true);
    s.set(a, false);
    expect(s.busy()).toBe(true);
    s.set(b, false);
    expect(s.busy()).toBe(false);
  });

  it("ignores repeated updates", () => {
    const s = new LoadingService();
    const a = Symbol("a");
    s.set(a, false);
    s.set(a, true);
    s.set(a, true);
    s.set(a, false);
    expect(s.busy()).toBe(false);
  });
});

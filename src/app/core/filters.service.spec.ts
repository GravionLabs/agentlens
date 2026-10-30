// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { FiltersService, validateLast, validateRange } from "./filters.service";

const KEY = "agentlens.filters";

function service(): FiltersService {
  TestBed.resetTestingModule();
  return TestBed.inject(FiltersService);
}
const stored = () => JSON.parse(localStorage.getItem(KEY)!);

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
beforeEach(() => {
  localStorage.clear();
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-10-30T12:00:00Z"));
});
afterEach(() => vi.useRealTimers());

describe("FiltersService", () => {
  it("sends a preset as given and all time as no bound", () => {
    const f = service();
    expect(f.payload().since).toBe("30d");
    f.setSince("");
    expect(f.payload()).toMatchObject({ since: null, until: null });
    expect(f.windowLabel()).toBe("all time");
  });

  it("stores a relative window as last and unit and renders it from now", () => {
    const f = service();
    expect(f.setLast(6, "h")).toBeNull();
    expect(stored()).toMatchObject({ last: 6, unit: "h" });
    expect(stored().from).toBeUndefined();
    expect(f.payload()).toMatchObject({ since: "2026-10-30T06:00:00.000Z", until: null });
    expect(f.windowLabel()).toBe("last 6 hours");
  });

  it("recomputes a relative window on load and on refresh", () => {
    service().setLast(2, "d");
    vi.setSystemTime(new Date("2026-11-01T12:00:00Z"));
    const f = service();
    expect(f.payload().since).toBe("2026-10-30T12:00:00.000Z");
    vi.setSystemTime(new Date("2026-11-02T12:00:00Z"));
    f.bump();
    expect(f.payload().since).toBe("2026-10-31T12:00:00.000Z");
  });

  it("stores an absolute range as its two instants and keeps it on load", () => {
    const f = service();
    expect(
      f.setRange(new Date("2026-10-01T00:00:00Z"), new Date("2026-10-08T00:00:00Z")),
    ).toBeNull();
    expect(stored()).toMatchObject({
      from: "2026-10-01T00:00:00.000Z",
      to: "2026-10-08T00:00:00.000Z",
    });
    expect(stored().last).toBeUndefined();
    vi.setSystemTime(new Date("2027-03-01T12:00:00Z"));
    const g = service();
    expect(g.payload()).toMatchObject({
      since: "2026-10-01T00:00:00.000Z",
      until: "2026-10-08T00:00:00.000Z",
    });
    expect(g.windowLabel()).toMatch(/^Oct 1, 2026 – Oct 8, 2026$/);
  });

  it("labels a range of the current year without the year", () => {
    const f = service();
    f.setRange(new Date("2026-10-15T12:00:00Z"), new Date("2026-10-20T12:00:00Z"));
    expect(f.windowLabel()).toBe("Oct 15 – Oct 20");
  });

  it("rejects invalid input and keeps the last valid filter", () => {
    const f = service();
    f.setLast(3, "d");
    expect(f.setLast(0, "d")).toMatch(/greater than zero/);
    expect(f.setLast(-2, "h")).toMatch(/greater than zero/);
    expect(f.setLast(NaN, "h")).toMatch(/number/);
    expect(f.last()).toEqual({ last: 3, unit: "d" });
    f.setRange(new Date("2026-10-01"), new Date("2026-10-02"));
    expect(f.setRange(new Date("2026-10-05"), new Date("2026-10-03"))).toMatch(/after the start/);
    expect(f.setRange(null, new Date("2026-10-03"))).toMatch(/start and an end/);
    expect(f.range()!.from).toBe("2026-10-01T00:00:00.000Z");
  });

  it("returns to a preset and drops the custom window", () => {
    const f = service();
    f.setLast(3, "d");
    f.setSince("7d");
    expect(f.periodChoice()).toBe("7d");
    expect(stored().last).toBeUndefined();
    expect(f.payload().since).toBe("7d");
  });

  it("reads an old stored filter and ignores a damaged custom window", () => {
    localStorage.setItem(KEY, JSON.stringify({ since: "90d", sources: [], project: "" }));
    expect(service().payload().since).toBe("90d");
    localStorage.setItem(KEY, JSON.stringify({ since: "7d", last: -1, unit: "x", from: "nope" }));
    const f = service();
    expect(f.periodChoice()).toBe("7d");
  });

  it("validates the pure inputs", () => {
    expect(validateLast(1.5, "d")).not.toBeNull();
    expect(validateLast(1, "w")).not.toBeNull();
    expect(validateLast(1, "h")).toBeNull();
    expect(validateRange(new Date("2026-01-01"), new Date("2026-01-01"))).not.toBeNull();
  });
});

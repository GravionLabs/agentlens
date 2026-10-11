import { beforeEach, describe, expect, it, vi } from "vitest";
import { ZoomService, clampZoom } from "./zoom.service";

describe("clampZoom", () => {
  it("stays within the range and on the step", () => {
    expect(clampZoom(0.1)).toBe(0.7);
    expect(clampZoom(5)).toBe(1.6);
    expect(clampZoom(1.0000001)).toBe(1);
    expect(clampZoom(1.04)).toBe(1);
    expect(clampZoom(1.06)).toBe(1.1);
  });
});

describe("ZoomService", () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    });
  });

  it("steps, resets and remembers the factor", () => {
    const zoom = new ZoomService();
    zoom.zoomIn();
    zoom.zoomIn();
    expect(zoom.factor()).toBe(1.2);
    expect(new ZoomService().factor()).toBe(1.2);
    zoom.zoomOut();
    expect(zoom.factor()).toBe(1.1);
    zoom.reset();
    expect(zoom.factor()).toBe(1);
  });

  it("does not go beyond the limits", () => {
    const zoom = new ZoomService();
    for (let i = 0; i < 20; i++) zoom.zoomIn();
    expect(zoom.factor()).toBe(1.6);
    for (let i = 0; i < 20; i++) zoom.zoomOut();
    expect(zoom.factor()).toBe(0.7);
  });

  it("handles Ctrl plus, minus and zero", () => {
    const zoom = new ZoomService();
    const key = (k: string, ctrl = true) =>
      zoom.handleKey({
        key: k,
        ctrlKey: ctrl,
        metaKey: false,
        altKey: false,
        preventDefault: () => undefined,
      } as KeyboardEvent);
    expect(key("=")).toBe(true);
    expect(zoom.factor()).toBe(1.1);
    expect(key("-")).toBe(true);
    expect(key("0")).toBe(true);
    expect(zoom.factor()).toBe(1);
    expect(key("a")).toBe(false);
    expect(key("=", false)).toBe(false);
  });
});

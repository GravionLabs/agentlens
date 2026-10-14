// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import {
  ESTIMATED_FALLBACK,
  createPaint,
  faintColor,
  hatchPattern,
  resolveColor,
  type ColorContext,
  type HatchCanvas,
} from "./hatch";

function mockCanvas() {
  const calls: string[] = [];
  const g = {
    fillStyle: "" as unknown,
    strokeStyle: "" as unknown,
    lineWidth: 0,
    globalAlpha: 1,
    alphaAtFill: -1,
    fillRect(...args: number[]) {
      calls.push(`fillRect ${args.join(",")}`);
      g.alphaAtFill = g.globalAlpha;
    },
    beginPath: () => calls.push("beginPath"),
    moveTo: () => calls.push("moveTo"),
    lineTo: () => calls.push("lineTo"),
    stroke: () => calls.push("stroke"),
  };
  const canvas: HatchCanvas = { width: 0, height: 0, getContext: () => g };
  return { canvas, g, calls };
}

describe("hatchPattern", () => {
  it("draws a faint tile with solid stripes and returns the pattern of the target", () => {
    const { canvas, g, calls } = mockCanvas();
    const pattern = {};
    const createPattern = vi.fn(() => pattern);
    const out = hatchPattern({ createPattern }, "#123456", () => canvas);
    expect(out).toBe(pattern);
    expect(createPattern).toHaveBeenCalledWith(canvas, "repeat");
    expect(canvas.width).toBe(8);
    expect(g.fillStyle).toBe("#123456");
    expect(g.alphaAtFill).toBe(0.35);
    expect(calls).toContain("fillRect 0,0,8,8");
    expect(g.globalAlpha).toBe(1);
    expect(g.lineWidth).toBe(3);
    expect(g.strokeStyle).toBe("#123456");
    expect(calls.filter((c) => c === "stroke")).toHaveLength(3);
  });

  it("draws the tile at the device pixel ratio and scales the pattern back", () => {
    const { canvas, g, calls } = mockCanvas();
    const pattern = { setTransform: vi.fn() };
    hatchPattern({ createPattern: () => pattern }, "red", () => canvas, 2);
    expect(canvas.width).toBe(16);
    expect(g.lineWidth).toBe(6);
    expect(calls).toContain("fillRect 0,0,16,16");
    expect(pattern.setTransform).toHaveBeenCalledWith({ a: 0.5, d: 0.5 });
  });

  it("does not scale the pattern at ratio 1", () => {
    const { canvas } = mockCanvas();
    const pattern = { setTransform: vi.fn() };
    hatchPattern({ createPattern: () => pattern }, "red", () => canvas, 1);
    expect(pattern.setTransform).not.toHaveBeenCalled();
  });

  it("returns null without a 2d context", () => {
    const canvas: HatchCanvas = { width: 0, height: 0, getContext: () => null };
    expect(hatchPattern({ createPattern: vi.fn() }, "red", () => canvas)).toBeNull();
  });
});

describe("resolveColor", () => {
  it("reads a variable from the host and passes plain colors on", () => {
    const host = document.createElement("div");
    host.style.setProperty("--c-test", " #123456 ");
    document.body.append(host);
    expect(resolveColor(host, "var(--c-test)")).toBe("#123456");
    expect(resolveColor(host, "#abcdef")).toBe("#abcdef");
    expect(resolveColor(host, "color-mix(in srgb, var(--c-test) 18%, transparent)")).toBe(
      "color-mix(in srgb, #123456 18%, transparent)",
    );
    host.remove();
  });
});

describe("createPaint", () => {
  const host = () => {
    const el = document.createElement("div");
    document.body.append(el);
    return el;
  };
  const ctxWith = (createPattern: () => unknown): ColorContext => ({
    dataIndex: 0,
    chart: { ctx: { createPattern } },
  });
  // jsdom has no canvas: give the offscreen tile a 2d context
  const stubCanvas = () =>
    vi
      .spyOn(document, "createElement")
      .mockImplementation(() => mockCanvas().canvas as unknown as HTMLElement);

  it("returns the resolved plain color when not estimated, without a pattern", () => {
    const el = host();
    el.style.setProperty("--c-a", "#111111");
    const createPattern = vi.fn();
    const paint = createPaint(el, () => 1);
    expect(paint(ctxWith(createPattern), "var(--c-a)", false)).toBe("#111111");
    expect(createPattern).not.toHaveBeenCalled();
  });

  it("builds the pattern once per resolved color and reuses it", () => {
    const el = host();
    el.style.setProperty("--c-a", "#111111");
    const spy = stubCanvas();
    const pattern = {};
    const createPattern = vi.fn(() => pattern);
    const paint = createPaint(el, () => 1);
    expect(paint(ctxWith(createPattern), "var(--c-a)", true)).toBe(pattern);
    expect(paint(ctxWith(createPattern), "var(--c-a)", true)).toBe(pattern);
    expect(createPattern).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });

  it("rebuilds the pattern when the variable changes (theme change) and when the ratio changes", () => {
    const el = host();
    el.style.setProperty("--c-a", "#111111");
    const spy = stubCanvas();
    const createPattern = vi.fn(() => ({}));
    let ratio = 1;
    const paint = createPaint(el, () => ratio);
    paint(ctxWith(createPattern), "var(--c-a)", true);
    el.style.setProperty("--c-a", "#eeeeee");
    paint(ctxWith(createPattern), "var(--c-a)", true);
    expect(createPattern).toHaveBeenCalledTimes(2);
    ratio = 2;
    paint(ctxWith(createPattern), "var(--c-a)", true);
    expect(createPattern).toHaveBeenCalledTimes(3);
    spy.mockRestore();
  });

  it("never draws an estimated value like a measured one when the pattern cannot be built", () => {
    const el = host();
    el.style.setProperty("--c-a", "#111111");
    const spy = vi
      .spyOn(document, "createElement")
      .mockImplementation(() => ({ getContext: () => null }) as unknown as HTMLElement);
    const paint = createPaint(el, () => 1);
    const out = paint(
      ctxWith(() => ({})),
      "var(--c-a)",
      true,
    );
    expect(out).toBe(faintColor("#111111"));
    expect(out).not.toBe("#111111");
    spy.mockRestore();
  });

  it("uses the visible fallback for an estimated value without a resolvable color", () => {
    const paint = createPaint(host(), () => 1);
    expect(
      paint(
        ctxWith(() => ({})),
        "var(--c-missing)",
        true,
      ),
    ).toBe(ESTIMATED_FALLBACK);
  });
});

// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { hatchPattern, resolveColor, type HatchCanvas } from "./hatch";

function mockCanvas() {
  const calls: string[] = [];
  const g = {
    fillStyle: "",
    strokeStyle: "",
    lineWidth: 0,
    globalAlpha: 1,
    fillRect: () => calls.push("fillRect"),
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
    expect(g.strokeStyle).toBe("#123456");
    expect(calls.filter((c) => c === "stroke")).toHaveLength(3);
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

/*
 * Hatched fill for estimated values on a chart.js canvas (spike #350), used by the timeline.
 * HxChart passes dataset colors through untouched, so a scriptable color returning a CanvasPattern works without a
 * plugin. HxChart redraws on a theme change with the same data object, so the colors are scriptable and the pattern
 * is rebuilt when the resolved CSS color differs.
 */

/** The part of a canvas that the hatch needs; lets the tests pass a mock. */
export interface HatchCanvas {
  width: number;
  height: number;
  getContext(id: "2d"): HatchContext | null;
}
export interface HatchContext {
  fillStyle: unknown;
  strokeStyle: unknown;
  lineWidth: number;
  fillRect(x: number, y: number, w: number, h: number): void;
  beginPath(): void;
  moveTo(x: number, y: number): void;
  lineTo(x: number, y: number): void;
  stroke(): void;
  globalAlpha: number;
}

const TILE = 8;
const WIDTH = 3;
const FAINT = 0.35;

/** What a pattern needs to be scaled back to CSS pixels on a high-DPI canvas. */
interface ScalablePattern {
  setTransform?(matrix: { a: number; d: number }): void;
}

/**
 * Diagonal hatch in `color`, like the SVG `<pattern>`: a faint background (35%) with solid 45 degree stripes.
 * The tile is drawn at `dpr` times the size (sharp stripes on a high-DPI screen) and the pattern is scaled back
 * by 1/dpr, because chart.js already scales its context by the device pixel ratio.
 * `null` when no offscreen canvas is available.
 */
export function hatchPattern(
  target: { createPattern(image: never, repetition: string): unknown },
  color: string,
  makeCanvas: () => HatchCanvas = () => document.createElement("canvas") as unknown as HatchCanvas,
  dpr = 1,
): unknown {
  const k = dpr > 0 ? dpr : 1;
  const size = Math.round(TILE * k);
  const tile = makeCanvas();
  tile.width = size;
  tile.height = size;
  const g = tile.getContext("2d");
  if (!g) return null;
  g.fillStyle = color;
  g.globalAlpha = FAINT;
  g.fillRect(0, 0, size, size);
  g.globalAlpha = 1;
  g.strokeStyle = color;
  g.lineWidth = WIDTH * k;
  // three diagonals so that the stripes meet at the tile edges
  for (const offset of [-size, 0, size]) {
    g.beginPath();
    g.moveTo(offset, size);
    g.lineTo(offset + size, 0);
    g.stroke();
  }
  const pattern = target.createPattern(tile as never, "repeat") as ScalablePattern | null;
  if (pattern && k !== 1) pattern.setTransform?.({ a: 1 / k, d: 1 / k });
  return pattern;
}

/** What the scriptable color gets from chart.js. */
export interface ColorContext {
  dataIndex: number;
  chart: { ctx: { createPattern(image: never, repetition: string): unknown } };
}

/** Resolves a CSS color or variable to a color for the canvas. */
export type Paint = (ctx: ColorContext, color: string, estimated: boolean) => unknown;

/** Replaces every `var(--name)` in `color` by its value on `host` (so a theme change gives the new color). */
export function resolveColor(host: Element, color: string): string {
  if (!color.includes("var(")) return color;
  const style = getComputedStyle(host);
  return color
    .replace(/var\((--[\w-]+)\)/g, (_, name: string) => style.getPropertyValue(name).trim())
    .trim();
}

/** Shown for an estimated value when no color can be resolved: visible, and not mistaken for a measured one. */
export const ESTIMATED_FALLBACK = "rgba(128, 128, 128, 0.45)";

/** The color of a hatch that could not be built (no offscreen canvas): the color at 45%, like the faint tile. */
export function faintColor(color: string): string {
  return `color-mix(in srgb, ${color} 45%, transparent)`;
}

/**
 * A `Paint` for `host`: the plain color, or the hatch pattern (cached per resolved color and pixel ratio) when
 * estimated. An estimated value is never drawn like a measured one: without a resolvable color it gets
 * `ESTIMATED_FALLBACK`, without an offscreen canvas the faint color.
 */
export function createPaint(
  host: Element,
  dpr: () => number = () => globalThis.devicePixelRatio || 1,
): Paint {
  const cache = new Map<string, unknown>();
  return (ctx, color, estimated) => {
    const resolved = resolveColor(host, color);
    if (!estimated) return resolved;
    if (!resolved) return ESTIMATED_FALLBACK;
    const ratio = dpr();
    const key = `${resolved}@${ratio}`;
    const cached = cache.get(key);
    if (cached) return cached;
    const pattern = hatchPattern(ctx.chart.ctx, resolved, undefined, ratio) ?? faintColor(resolved);
    cache.set(key, pattern);
    return pattern;
  };
}

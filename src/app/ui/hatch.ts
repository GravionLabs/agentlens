/*
 * Hatched fill for estimated values on a chart.js canvas (spike #350), shared by the stacked bars and the timeline.
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

/**
 * Diagonal hatch in `color`, like the SVG `<pattern>`: a faint background (35%) with solid 45 degree stripes.
 * `null` when no offscreen canvas is available.
 */
export function hatchPattern(
  target: { createPattern(image: never, repetition: string): unknown },
  color: string,
  makeCanvas: () => HatchCanvas = () => document.createElement("canvas") as unknown as HatchCanvas,
): unknown {
  const tile = makeCanvas();
  tile.width = TILE;
  tile.height = TILE;
  const g = tile.getContext("2d");
  if (!g) return null;
  g.fillStyle = color;
  g.globalAlpha = 0.35;
  g.fillRect(0, 0, TILE, TILE);
  g.globalAlpha = 1;
  g.strokeStyle = color;
  g.lineWidth = 3;
  // three diagonals so that the stripes meet at the tile edges
  for (const offset of [-TILE, 0, TILE]) {
    g.beginPath();
    g.moveTo(offset, TILE);
    g.lineTo(offset + TILE, 0);
    g.stroke();
  }
  return target.createPattern(tile as never, "repeat");
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

/** A `Paint` for `host`: the plain color, or the hatch pattern (cached per resolved color) when estimated. */
export function createPaint(host: Element): Paint {
  const cache = new Map<string, unknown>();
  return (ctx, color, estimated) => {
    const resolved = resolveColor(host, color);
    if (!estimated || !resolved) return resolved;
    const cached = cache.get(resolved);
    if (cached) return cached;
    const pattern = hatchPattern(ctx.chart.ctx, resolved) ?? resolved;
    cache.set(resolved, pattern);
    return pattern;
  };
}

import type { MarkerKind, TimelineMarker, TimelinePoint } from "../../core/types";

export const WIDTH = 1000;
export const HEIGHT = 320;
export const MARGIN = { left: 64, right: 16, top: 16, bottom: 28 };

export interface Scale {
  x: (request: number) => number;
  y: (tokens: number) => number;
  max: number;
  last: number;
}

/** Scales for a chart of `points`: request number to x, context tokens to y. */
export function scales(points: TimelinePoint[]): Scale {
  const last = Math.max(1, points.length > 0 ? points[points.length - 1].request : 1);
  const first = points.length > 0 ? points[0].request : 0;
  const max = Math.max(1, ...points.map((p) => p.context_tokens));
  const w = WIDTH - MARGIN.left - MARGIN.right;
  const h = HEIGHT - MARGIN.top - MARGIN.bottom;
  const span = Math.max(1, last - first);
  return {
    x: (r) => MARGIN.left + ((r - first) / span) * w,
    y: (t) => MARGIN.top + h - (t / max) * h,
    max,
    last,
  };
}

/** The line through all points as an SVG path. */
export function linePath(points: TimelinePoint[], s: Scale): string {
  return points
    .map(
      (p, i) =>
        `${i === 0 ? "M" : "L"}${s.x(p.request).toFixed(1)},${s.y(p.context_tokens).toFixed(1)}`,
    )
    .join(" ");
}

/** The same line closed down to the x axis, for the area under it. */
export function areaPath(points: TimelinePoint[], s: Scale): string {
  if (points.length === 0) return "";
  const base = (HEIGHT - MARGIN.bottom).toFixed(1);
  const first = s.x(points[0].request).toFixed(1);
  const last = s.x(points[points.length - 1].request).toFixed(1);
  return `${linePath(points, s)} L${last},${base} L${first},${base} Z`;
}

export function markerLabel(kind: MarkerKind): string {
  switch (kind.type) {
    case "skill":
      return `skill ${kind.name} (${kind.trigger})`;
    case "compaction":
      return "compaction";
    case "tool-error":
      return `tool error: ${kind.tool}`;
    case "hook-failed":
      return `hook failed: ${kind.hook}`;
    case "permission-prompt":
      return kind.wait_ms === null
        ? "permission prompt"
        : `permission prompt (${(kind.wait_ms / 1000).toFixed(1)} s)`;
  }
}

export const MARKER_COLORS: Record<MarkerKind["type"], string> = {
  skill: "var(--c-load)",
  compaction: "var(--bad)",
  "tool-error": "var(--warn)",
  "hook-failed": "var(--c-carry)",
  "permission-prompt": "var(--muted)",
};

/** Markers sit between two requests: the x of the gap before request `request`. */
export function markerX(m: TimelineMarker, s: Scale, firstRequest: number): number {
  const r = Math.max(m.request - 0.5, firstRequest);
  return s.x(r);
}

/** Round tick values for the y axis. */
export function yTicks(max: number): number[] {
  return [0, 0.5, 1].map((f) => Math.round(max * f));
}

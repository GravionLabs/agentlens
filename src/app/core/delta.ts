import { day, number, percent, tokens, usd } from "./format";
import type { Delta, TrendWindows } from "./types";

/** How a figure is written: a count, a token count, money, or a share (0..1, changes in points). */
export type DeltaKind = "count" | "tokens" | "usd" | "ratio";

/** Which direction is good: `null` when neither is (the delta is not colored). */
export type Good = "up" | "down" | null;

export interface DeltaView {
  direction: "up" | "down" | "flat" | "none";
  arrow: string;
  /** `+$1.20 (+14.3%)`; `n/a` where a window is empty. A tilde marks an estimate. */
  text: string;
  tone: "good" | "bad" | "neutral";
}

const ARROWS = { up: "▲", down: "▼", flat: "–", none: "" };

/** A per-day value below 10 keeps a decimal; counts are whole numbers otherwise. */
function count(n: number, perDay: boolean): string {
  return perDay && Math.abs(n) < 10 ? n.toFixed(1) : number(n);
}

function figure(n: number, kind: DeltaKind, estimated: boolean, perDay: boolean): string {
  switch (kind) {
    case "usd":
      return usd(n, estimated);
    case "tokens":
      return tokens(n, estimated);
    case "ratio":
      return `${estimated ? "~" : ""}${(n * 100).toFixed(1)} pts`;
    case "count":
      return (estimated ? "~" : "") + count(n, perDay);
  }
}

/** The value of a figure as a stat card shows it; `n/a` when the window has none. */
export function deltaValue(
  value: number | null,
  kind: DeltaKind,
  estimated = false,
  perDay = false,
): string {
  if (value === null) return "n/a";
  if (kind === "ratio") return percent(value);
  return figure(value, kind, estimated, perDay);
}

/** The change of a figure with its direction; color only where a direction is good or bad. */
export function deltaView(d: Delta, kind: DeltaKind, good: Good = null, perDay = false): DeltaView {
  if (d.change === null) return { direction: "none", arrow: "", text: "n/a", tone: "neutral" };
  const est = !d.measured;
  const flat = d.change === 0;
  const direction = flat ? "flat" : d.change > 0 ? "up" : "down";
  const sign = direction === "up" ? "+" : direction === "down" ? "-" : "";
  const abs = `${sign}${figure(Math.abs(d.change), kind, est, perDay)}`;
  const pct =
    d.change_pct === null
      ? "n/a"
      : `${d.change_pct > 0 ? "+" : d.change_pct < 0 ? "-" : ""}${est ? "~" : ""}${Math.abs(d.change_pct).toFixed(1)}%`;
  const tone =
    direction === "flat" || good === null ? "neutral" : direction === good ? "good" : "bad";
  return { direction, arrow: ARROWS[direction], text: `${abs} (${pct})`, tone };
}

/** The sum of figures, e.g. the loads of all skills; empty (`change` null) when a window has no sessions. */
export function sumDeltas(items: Delta[], windows: TrendWindows): Delta {
  const before = items.reduce((a, d) => a + (d.before ?? 0), 0);
  const after = items.reduce((a, d) => a + (d.after ?? 0), 0);
  const empty = windows.current.sessions === 0 || windows.against.sessions === 0;
  return {
    before,
    after,
    change: empty ? null : after - before,
    change_pct: empty || before === 0 ? null : ((after - before) / before) * 100,
    measured: items.every((d) => d.measured),
  };
}

/** One line naming the comparison window. */
export function trendNote(w: TrendWindows): string {
  const against = `${day(w.against.since)} – ${day(w.against.until)}`;
  const per = w.normalized ? "; the windows differ in length, so counts are per day" : "";
  return `vs. previous period (${against}, ${w.against.sessions} sessions)${per}`;
}

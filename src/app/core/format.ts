import { tokenFormat } from "./token-format.service";

/** `1,234`; `n/a` for a value a client does not record. */
export function number(n: number | null | undefined): string {
  if (n === null || n === undefined) return "n/a";
  return Math.round(n).toLocaleString("en-US");
}

/** Token counts: `~` marks an estimate, like the CLI. */
export function tokens(n: number | null | undefined, estimated = false): string {
  if (n === null || n === undefined) return "n/a";
  const text = tokenFormat() === "rounded" ? rounded(n) : number(n);
  return (estimated ? "~" : "") + text;
}

const UNITS: [number, string][] = [
  [1e3, "k"],
  [1e6, "M"],
  [1e9, "B"],
];

/** `2.11 k`, `1.55 M`, `3.20 B`; below 1,000 the number as it is. */
function rounded(n: number): string {
  const abs = Math.abs(n);
  if (abs < 1e3) return number(n);
  let i = UNITS.length - 1;
  while (i > 0 && abs < UNITS[i][0]) i--;
  let value = (abs / UNITS[i][0]).toFixed(2);
  if (Number(value) >= 1000 && i < UNITS.length - 1) {
    i++;
    value = (abs / UNITS[i][0]).toFixed(2);
  }
  return `${n < 0 ? "-" : ""}${value} ${UNITS[i][1]}`;
}

/** `12.3M`, `4.5k` for chart labels. */
export function compact(n: number): string {
  const abs = Math.abs(n);
  if (abs >= 1e9) return `${(n / 1e9).toFixed(abs >= 1e10 ? 0 : 1)}B`;
  if (abs >= 1e6) return `${(n / 1e6).toFixed(abs >= 1e7 ? 0 : 1)}M`;
  if (abs >= 1e3) return `${(n / 1e3).toFixed(abs >= 1e4 ? 0 : 1)}k`;
  return String(Math.round(n));
}

export function usd(v: number | null | undefined, estimated = false): string {
  if (v === null || v === undefined) return "n/a";
  return `${estimated ? "~" : ""}$${v.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

export function percent(v: number | null | undefined, digits = 1): string {
  if (v === null || v === undefined) return "n/a";
  return `${(v * 100).toFixed(digits)}%`;
}

export function minutes(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "n/a";
  return (ms / 60_000).toFixed(1);
}

export function seconds(ms: number): string {
  return (ms / 1000).toFixed(1);
}

/** The date part of an RFC 3339 instant, or `-`. */
export function day(ts: string | null | undefined): string {
  return ts ? ts.slice(0, 10) : "-";
}

export function shortId(id: string): string {
  return id.length > 8 ? id.slice(0, 8) : id;
}

export function sourceLabel(id: string): string {
  switch (id) {
    case "claude-code":
      return "Claude Code";
    case "opencode":
      return "OpenCode";
    case "copilot-cli":
      return "Copilot CLI";
    case "vscode-copilot":
      return "VS Code Copilot";
    default:
      return id;
  }
}

/** The last two segments of a path (`gravionlabs/ariadne`); the full path belongs in a tooltip. */
export function shortPath(path: string | null | undefined): string {
  if (!path) return "-";
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length <= 2 ? path : parts.slice(-2).join("/");
}

/** Shortens values that are paths and leaves everything else (model names, issue numbers) as it is. */
export function pathLabel(value: string): string {
  return value.startsWith("/") || /^[A-Za-z]:[\\/]/.test(value) ? shortPath(value) : value;
}

/** The time of day (UTC) of an RFC 3339 instant, or `-`. */
export function timeOfDay(ts: string | null | undefined): string {
  return ts && ts.length >= 19 ? ts.slice(11, 19) : "-";
}

import { Injectable, computed, signal } from "@angular/core";
import type { Filters, SourceId } from "./types";

const KEY = "agentlens.filters";

export const PERIODS: { value: string; label: string }[] = [
  { value: "24h", label: "Last 24 hours" },
  { value: "7d", label: "Last 7 days" },
  { value: "30d", label: "Last 30 days" },
  { value: "90d", label: "Last 90 days" },
  { value: "", label: "All time" },
];

export type LastUnit = "h" | "d";

export const LAST_UNITS: { value: LastUnit; label: string }[] = [
  { value: "h", label: "hours" },
  { value: "d", label: "days" },
];

/** The value of the period select that opens the custom inputs. */
export const CUSTOM = "custom";

const UNIT_MS: Record<LastUnit, number> = { h: 3_600_000, d: 86_400_000 };

/** A relative window: the last N hours or days, counted from now. */
export interface LastWindow {
  last: number;
  unit: LastUnit;
}

/** An absolute window: two instants (ISO 8601). */
export interface RangeWindow {
  from: string;
  to: string;
}

interface Stored {
  since?: string;
  last?: number;
  unit?: LastUnit;
  from?: string;
  to?: string;
  sources?: SourceId[];
  project?: string;
  roles?: string[];
  compare?: boolean;
}

function load(): Stored | null {
  try {
    const raw = localStorage.getItem(KEY);
    const value = raw ? (JSON.parse(raw) as unknown) : null;
    return value && typeof value === "object" ? (value as Stored) : null;
  } catch {
    return null;
  }
}

/** Why a "last N" input is not usable, or null. */
export function validateLast(last: unknown, unit: unknown): string | null {
  if (unit !== "h" && unit !== "d") return "Choose hours or days.";
  if (typeof last !== "number" || !Number.isFinite(last)) return "Enter a number.";
  if (!Number.isInteger(last) || last <= 0) return "Enter a whole number greater than zero.";
  return null;
}

/** Why a date range is not usable, or null. */
export function validateRange(from: Date | null, to: Date | null): string | null {
  if (!from || !to || Number.isNaN(from.getTime()) || Number.isNaN(to.getTime())) {
    return "Choose a start and an end.";
  }
  if (to.getTime() <= from.getTime()) return "The end must be after the start.";
  return null;
}

function day(iso: string, now: Date): string {
  const d = new Date(iso);
  const year = d.getFullYear() === now.getFullYear() ? {} : { year: "numeric" as const };
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric", ...year });
}

/** The filter bar state, shared by every view and remembered between runs. */
@Injectable({ providedIn: "root" })
export class FiltersService {
  private readonly stored = load();

  /** The preset in use; "" is all time. Ignored while a custom window is active. */
  readonly since = signal(this.stored?.since ?? "30d");
  /** The relative custom window, if one is active. */
  readonly last = signal<LastWindow | null>(
    this.stored && validateLast(this.stored.last, this.stored.unit) === null
      ? { last: this.stored.last as number, unit: this.stored.unit as LastUnit }
      : null,
  );
  /** The absolute custom window, if one is active. */
  readonly range = signal<RangeWindow | null>(
    this.last() === null &&
      validateRange(new Date(this.stored?.from ?? NaN), new Date(this.stored?.to ?? NaN)) === null
      ? { from: this.stored!.from!, to: this.stored!.to! }
      : null,
  );
  /** Empty means all sources. */
  readonly sources = signal<SourceId[]>(this.stored?.sources ?? []);
  readonly project = signal(this.stored?.project ?? "");
  /** Empty means all roles. */
  readonly roles = signal<string[]>(this.stored?.roles ?? []);

  /** Whether the views show the change against the previous period (off by default). */
  readonly compare = signal(this.stored?.compare === true);

  /** Raised by the refresh button so every analysis runs again. */
  readonly generation = signal(0);
  /** The instant a relative window is counted from; renewed on every refresh. */
  private readonly now = signal(Date.now());

  /** What the period select shows: a preset value or {@link CUSTOM}. */
  readonly periodChoice = computed(() => (this.last() || this.range() ? CUSTOM : this.since()));

  /** The active window in words, for the page header. */
  readonly windowLabel = computed(() => {
    const last = this.last();
    if (last) return `last ${last.last} ${last.unit === "h" ? "hours" : "days"}`;
    const range = this.range();
    if (range) {
      const now = new Date(this.now());
      return `${day(range.from, now)} \u2013 ${day(range.to, now)}`;
    }
    return PERIODS.find((p) => p.value === this.since())?.label.toLowerCase() ?? this.since();
  });

  /** What the commands receive. */
  readonly payload = computed<Filters>(() => {
    const last = this.last();
    const range = this.range();
    let since: string | null = this.since() || null;
    let until: string | null = null;
    if (last) {
      since = new Date(this.now() - last.last * UNIT_MS[last.unit]).toISOString();
    } else if (range) {
      since = range.from;
      until = range.to;
    }
    return {
      since,
      until,
      sources: this.sources(),
      project: this.project().trim() || null,
      roles: this.roles(),
    };
  });

  bump(): void {
    this.now.set(Date.now());
    this.generation.update((g) => g + 1);
  }

  setSince(value: string): void {
    this.since.set(value);
    this.last.set(null);
    this.range.set(null);
    this.persist();
  }

  /** Uses the last N hours or days; returns the reason when the input is invalid (the filter stays). */
  setLast(last: unknown, unit: unknown): string | null {
    const error = validateLast(last, unit);
    if (error) return error;
    this.last.set({ last: last as number, unit: unit as LastUnit });
    this.range.set(null);
    this.now.set(Date.now());
    this.persist();
    return null;
  }

  /** Uses a date range; returns the reason when the input is invalid (the filter stays). */
  setRange(from: Date | null, to: Date | null): string | null {
    const error = validateRange(from, to);
    if (error) return error;
    this.range.set({ from: from!.toISOString(), to: to!.toISOString() });
    this.last.set(null);
    this.persist();
    return null;
  }

  setProject(value: string): void {
    this.project.set(value);
    this.persist();
  }

  setRoles(value: string[]): void {
    this.roles.set(value);
    this.persist();
  }

  setCompare(value: boolean): void {
    this.compare.set(value);
    this.persist();
  }

  private persist(): void {
    const last = this.last();
    const range = this.range();
    const value: Stored = {
      since: this.since(),
      ...(last ? { last: last.last, unit: last.unit } : {}),
      ...(range ? { from: range.from, to: range.to } : {}),
      sources: this.sources(),
      project: this.project(),
      roles: this.roles(),
      compare: this.compare(),
    };
    try {
      localStorage.setItem(KEY, JSON.stringify(value));
    } catch {
      // Remembering the filters is a convenience only.
    }
  }

  toggleSource(id: SourceId): void {
    this.sources.update((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));
    this.persist();
  }
}

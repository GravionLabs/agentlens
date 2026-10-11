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

interface Stored {
  since: string;
  sources: SourceId[];
  project: string;
}

function load(): Stored | null {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? (JSON.parse(raw) as Stored) : null;
  } catch {
    return null;
  }
}

/** The filter bar state, shared by every view and remembered between runs. */
@Injectable({ providedIn: "root" })
export class FiltersService {
  private readonly stored = load();

  readonly since = signal(this.stored?.since ?? "30d");
  /** Empty means all sources. */
  readonly sources = signal<SourceId[]>(this.stored?.sources ?? []);
  readonly project = signal(this.stored?.project ?? "");

  /** Raised by the refresh button so every analysis runs again. */
  readonly generation = signal(0);

  /** What the commands receive. */
  readonly payload = computed<Filters>(() => ({
    since: this.since() || null,
    sources: this.sources(),
    project: this.project().trim() || null,
  }));

  bump(): void {
    this.generation.update((g) => g + 1);
  }

  setSince(value: string): void {
    this.since.set(value);
    this.persist();
  }

  setProject(value: string): void {
    this.project.set(value);
    this.persist();
  }

  private persist(): void {
    const value: Stored = {
      since: this.since(),
      sources: this.sources(),
      project: this.project(),
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

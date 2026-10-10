import { Injectable, computed, signal } from "@angular/core";

/** Tracks which analyses are loading, so the shell can show one busy indicator. */
@Injectable({ providedIn: "root" })
export class LoadingService {
  private readonly active = signal<ReadonlySet<symbol>>(new Set());

  /** `true` while at least one analysis is loading. */
  readonly busy = computed(() => this.active().size > 0);

  set(id: symbol, loading: boolean): void {
    const has = this.active().has(id);
    if (loading === has) return;
    const next = new Set(this.active());
    if (loading) next.add(id);
    else next.delete(id);
    this.active.set(next);
  }
}

import { Injectable, signal } from "@angular/core";

export type WithoutKind = "skill" | "agent" | "mcp-server";

/** The `KIND:NAME` form the `without` parameter takes. */
export function withoutItem(kind: string, name: string): string {
  return `${kind}:${name}`;
}

/**
 * The items the user asked “what if removed” for, shown as chips in the cost view. Kept while the
 * application runs, not persisted, and separate from the filters so other views are not affected.
 */
@Injectable({ providedIn: "root" })
export class WithoutService {
  readonly items = signal<string[]>([]);

  has(kind: string, name: string): boolean {
    return this.items().includes(withoutItem(kind, name));
  }

  /** Adds the item; one that is already on the list stays where it is. */
  add(kind: string, name: string): void {
    const item = withoutItem(kind, name);
    this.items.update((l) => (l.includes(item) ? l : [...l, item]));
  }

  remove(item: string): void {
    this.items.update((l) => l.filter((i) => i !== item));
  }
}

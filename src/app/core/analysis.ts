import { DestroyRef, effect, inject, resource } from "@angular/core";
import { FiltersService } from "./filters.service";
import { Ipc } from "./ipc";
import { LoadingService } from "./loading.service";
import type { Answer } from "./types";

/**
 * Runs one analysis command and runs it again whenever the filters or the
 * extra arguments change, and stays idle while `enabled` is false. The result carries the data plus the warnings and
 * errors of reading the sessions.
 */
export function analysis<T>(
  command: string,
  extra: () => Record<string, unknown> = () => ({}),
  enabled: () => boolean = () => true,
) {
  const ipc = inject(Ipc);
  const filters = inject(FiltersService);
  const loading = inject(LoadingService);
  const id = Symbol(command);
  const res = resource({
    // Without params the resource stays idle.
    params: () =>
      enabled()
        ? { filters: filters.payload(), extra: extra(), generation: filters.generation() }
        : undefined,
    loader: async ({ params }) =>
      ipc.invoke<Answer<T>>(command, { filters: params.filters, ...params.extra }),
  });
  effect(() => loading.set(id, res.isLoading()));
  inject(DestroyRef).onDestroy(() => loading.set(id, false));
  return res;
}

/** A readable message for a failed command. */
export function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.cause ? messageOf(error.cause) : error.message;
  return "Something went wrong.";
}

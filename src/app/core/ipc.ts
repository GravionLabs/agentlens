import { Injectable, InjectionToken, inject } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";

/** Calls the commands of the Tauri backend. */
export abstract class Ipc {
  abstract invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}

@Injectable()
export class TauriIpc extends Ipc {
  override invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    return invoke<T>(command, args);
  }
}

const SAMPLES: Record<string, () => Promise<{ default: unknown }>> = {
  overview: () => import("./samples/overview.json"),
  skills: () => import("./samples/skills.json"),
  context: () => import("./samples/context.json"),
  unused: () => import("./samples/unused.json"),
  flow: () => import("./samples/flow.json"),
  cost: () => import("./samples/cost.json"),
  efficiency: () => import("./samples/efficiency.json"),
  compare: () => import("./samples/compare.json"),
  sessions: () => import("./samples/sessions.json"),
  timeline: () => import("./samples/timeline.json"),
};

/** Answers with the sample data; used when the page runs outside Tauri (`pnpm start`). */
@Injectable()
export class SampleIpc extends Ipc {
  override async invoke<T>(command: string): Promise<T> {
    const load = SAMPLES[command];
    if (!load) throw new Error(`no sample for the command ${command}`);
    // Like the real backend, the answer takes a moment (shows the loading state).
    await new Promise((resolve) => setTimeout(resolve, 600));
    return (await load()).default as T;
  }
}

export function runsInTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const IPC_FACTORY = () => (runsInTauri() ? new TauriIpc() : new SampleIpc());

export const SAMPLE_MODE = new InjectionToken<boolean>("sample mode", {
  providedIn: "root",
  factory: () => !runsInTauri(),
});

export function injectIpc(): Ipc {
  return inject(Ipc);
}

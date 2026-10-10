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
  update_check: () => Promise.resolve({ default: null }),
  update_install: () => Promise.resolve({ default: null }),
  refresh: () => Promise.resolve({ default: { data: null, warnings: 0, errors: [] } }),
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

/** Calls the development backend (`cargo run -p agentlens-app --example dev-backend`). */
export class HttpIpc extends Ipc {
  private readonly base: string;

  constructor(base: string) {
    super();
    this.base = base;
  }

  override async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    const res = await fetch(`${this.base}/${command}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args ?? {}),
    });
    const body: unknown = await res.json();
    if (!res.ok) throw new Error(typeof body === "string" ? body : "The backend failed.");
    return body as T;
  }
}

/** `?backend=http://127.0.0.1:1421` in the address selects the development backend. */
export function devBackend(): string | null {
  return BACKEND;
}

// Read once at start: the router rewrites the address and drops the query.
const BACKEND =
  typeof window === "undefined" ? null : new URLSearchParams(window.location.search).get("backend");

export function runsInTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const IPC_FACTORY = () => {
  const backend = devBackend();
  if (backend) return new HttpIpc(backend);
  return runsInTauri() ? new TauriIpc() : new SampleIpc();
};

export const SAMPLE_MODE = new InjectionToken<boolean>("sample mode", {
  providedIn: "root",
  factory: () => !runsInTauri() && !devBackend(),
});

export function injectIpc(): Ipc {
  return inject(Ipc);
}

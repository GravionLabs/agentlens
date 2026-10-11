import { Injectable, computed, inject, signal } from "@angular/core";
import { messageOf } from "./analysis";
import { Ipc } from "./ipc";
import type { UpdateInfo } from "./types";

export type UpdateState = "idle" | "checking" | "upToDate" | "available" | "installing" | "error";

/**
 * Checks for and installs application updates. Nothing is checked in the background: the network
 * request only happens when the user asks for it.
 */
@Injectable({ providedIn: "root" })
export class UpdateService {
  private readonly ipc = inject(Ipc);

  /** The running version; null until it is known (or when it cannot be read). */
  readonly version = signal<string | null>(null);
  readonly state = signal<UpdateState>("idle");
  readonly info = signal<UpdateInfo | null>(null);
  readonly error = signal<string | null>(null);
  readonly busy = computed(() => this.state() === "checking" || this.state() === "installing");

  async loadVersion(): Promise<void> {
    try {
      this.version.set(await this.ipc.invoke<string>("app_version"));
    } catch {
      this.version.set(null);
    }
  }

  async check(): Promise<void> {
    if (this.busy()) return;
    this.state.set("checking");
    this.error.set(null);
    try {
      const info = await this.ipc.invoke<UpdateInfo | null>("update_check");
      this.info.set(info);
      this.state.set(info ? "available" : "upToDate");
    } catch (error) {
      this.fail(error);
    }
  }

  /** Installs the update found by {@link check}. On success the app restarts, so nothing follows. */
  async install(): Promise<void> {
    if (this.state() !== "available") return;
    this.state.set("installing");
    this.error.set(null);
    try {
      await this.ipc.invoke("update_install");
    } catch (error) {
      this.fail(error);
    }
  }

  private fail(error: unknown): void {
    this.error.set(messageOf(error));
    this.state.set("error");
  }
}

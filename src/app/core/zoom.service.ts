import { Injectable, signal } from "@angular/core";

const KEY = "agentlens.zoom";
export const ZOOM_MIN = 0.7;
export const ZOOM_MAX = 1.6;
export const ZOOM_STEP = 0.1;

/** Clamps to the allowed range and rounds to a step, so repeated steps do not drift. */
export function clampZoom(value: number): number {
  const stepped = Math.round(value / ZOOM_STEP) * ZOOM_STEP;
  return Math.round(Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, stepped)) * 100) / 100;
}

/**
 * The size of the whole interface. In the application the webview is zoomed (like a browser's
 * Ctrl + / Ctrl -); the factor is remembered between runs when the storage allows it.
 */
@Injectable({ providedIn: "root" })
export class ZoomService {
  readonly factor = signal(1);

  constructor() {
    this.factor.set(this.read());
    void this.apply();
  }

  zoomIn(): void {
    this.set(this.factor() + ZOOM_STEP);
  }

  zoomOut(): void {
    this.set(this.factor() - ZOOM_STEP);
  }

  reset(): void {
    this.set(1);
  }

  /** Ctrl + / Ctrl - / Ctrl 0 (and the numpad variants). Returns whether the key was used. */
  handleKey(event: KeyboardEvent): boolean {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return false;
    if (event.key === "+" || event.key === "=") this.zoomIn();
    else if (event.key === "-" || event.key === "_") this.zoomOut();
    else if (event.key === "0") this.reset();
    else return false;
    event.preventDefault();
    return true;
  }

  private set(value: number): void {
    this.factor.set(clampZoom(value));
    try {
      localStorage.setItem(KEY, String(this.factor()));
    } catch {
      // The storage can be unavailable; the size then lasts until the app closes.
    }
    void this.apply();
  }

  private read(): number {
    try {
      const stored = Number(localStorage.getItem(KEY));
      return stored > 0 ? clampZoom(stored) : 1;
    } catch {
      return 1;
    }
  }

  private async apply(): Promise<void> {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
    try {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      await getCurrentWebview().setZoom(this.factor());
    } catch {
      // Without the permission or outside the app the size stays as it is.
    }
  }
}

import { Injectable, InjectionToken, inject, signal } from "@angular/core";
import { messageOf } from "./analysis";
import { FiltersService } from "./filters.service";
import { Ipc } from "./ipc";

export type ExportFormat = "json" | "html" | "csv";

/** What the backend reports after writing the file. */
export interface Exported {
  path: string;
  bytes: number;
  warnings: number;
  errors: string[];
}

export type ExportStatus =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "done"; path: string; warnings: number }
  | { kind: "error"; message: string };

/** Asks where to save; null when the user cancels. */
export type SaveDialog = (suggestedName: string, format: ExportFormat) => Promise<string | null>;

export const SAVE_DIALOG = new InjectionToken<SaveDialog>("save dialog", {
  providedIn: "root",
  factory: () => async (suggestedName, format) => {
    const { save } = await import("@tauri-apps/plugin-dialog");
    return save({ defaultPath: suggestedName, filters: [{ name: format, extensions: [format] }] });
  },
});

/**
 * Saves what a view shows to a file. The analysis runs in the backend with the filters of the
 * view; the file is only written to the place the user chose in the save dialog.
 */
@Injectable({ providedIn: "root" })
export class ExportService {
  private readonly ipc = inject(Ipc);
  private readonly filters = inject(FiltersService);
  private readonly dialog = inject(SAVE_DIALOG);

  readonly status = signal<ExportStatus>({ kind: "idle" });

  async export(
    view: string,
    format: ExportFormat,
    options?: Record<string, unknown>,
  ): Promise<void> {
    if (this.status().kind === "saving") return;
    this.status.set({ kind: "saving" });
    try {
      const name = await this.ipc.invoke<string>("export_name", { view, format });
      const path = await this.dialog(name, format);
      if (!path) {
        this.status.set({ kind: "idle" });
        return;
      }
      const done = await this.ipc.invoke<Exported>("export_view", {
        view,
        format,
        filters: this.filters.payload(),
        options: options ?? null,
        path,
      });
      this.status.set({ kind: "done", path: done.path, warnings: done.warnings });
    } catch (error) {
      this.status.set({ kind: "error", message: messageOf(error) });
    }
  }

  dismiss(): void {
    this.status.set({ kind: "idle" });
  }
}

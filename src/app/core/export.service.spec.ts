import { Injector, runInInjectionContext } from "@angular/core";
import { describe, expect, it, vi } from "vitest";
import { ExportService, SAVE_DIALOG, type SaveDialog } from "./export.service";
import { FiltersService } from "./filters.service";
import { Ipc } from "./ipc";

function create(
  invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>,
  dialog: SaveDialog,
) {
  const injector = Injector.create({
    providers: [
      { provide: Ipc, useValue: { invoke } },
      {
        provide: FiltersService,
        useValue: { payload: () => ({ since: "30d", sources: [], project: null }) },
      },
      { provide: SAVE_DIALOG, useValue: dialog },
    ],
  });
  return runInInjectionContext(injector, () => new ExportService());
}

describe("ExportService", () => {
  it("writes to the chosen file with the filters of the view", async () => {
    const invoke = vi.fn(async (command: string) =>
      command === "export_name"
        ? "agentlens-cost-2026-10-11.json"
        : { path: "/tmp/out.json", bytes: 12, warnings: 1, errors: [] },
    );
    const dialog = vi.fn(async () => "/tmp/out.json");
    const service = create(invoke, dialog);
    await service.export("cost", "json", { by: "project" });
    expect(dialog).toHaveBeenCalledWith("agentlens-cost-2026-10-11.json", "json");
    expect(invoke).toHaveBeenLastCalledWith("export_view", {
      view: "cost",
      format: "json",
      filters: { since: "30d", sources: [], project: null },
      options: { by: "project" },
      path: "/tmp/out.json",
    });
    expect(service.status()).toEqual({ kind: "done", path: "/tmp/out.json", warnings: 1 });
  });

  it("writes nothing when the dialog is canceled", async () => {
    const invoke = vi.fn(async () => "name.json");
    const service = create(invoke, async () => null);
    await service.export("skills", "json");
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(service.status()).toEqual({ kind: "idle" });
  });

  it("reports a failure instead of throwing", async () => {
    const service = create(
      async (command) => {
        if (command === "export_view") throw "cannot write /root/x: permission denied";
        return "name.json";
      },
      async () => "/root/x",
    );
    await service.export("skills", "json");
    expect(service.status()).toEqual({
      kind: "error",
      message: "cannot write /root/x: permission denied",
    });
  });
});

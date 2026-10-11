import { Injector, runInInjectionContext } from "@angular/core";
import { describe, expect, it, vi } from "vitest";
import { Ipc } from "./ipc";
import type { UpdateInfo } from "./types";
import { UpdateService } from "./update.service";

const newer: UpdateInfo = {
  version: "0.5.0",
  currentVersion: "0.4.0",
  notes: "Fixes",
  date: null,
};

function create(invoke: (command: string) => Promise<unknown>): UpdateService {
  const injector = Injector.create({
    providers: [{ provide: Ipc, useValue: { invoke } }],
  });
  return runInInjectionContext(injector, () => new UpdateService());
}

describe("UpdateService", () => {
  it("starts idle and does not check by itself", () => {
    const invoke = vi.fn();
    const service = create(invoke);
    expect(service.state()).toBe("idle");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("reports an available update", async () => {
    const service = create(async () => newer);
    await service.check();
    expect(service.state()).toBe("available");
    expect(service.info()).toEqual(newer);
  });

  it("reports when the application is up to date", async () => {
    const service = create(async () => null);
    await service.check();
    expect(service.state()).toBe("upToDate");
    expect(service.info()).toBeNull();
  });

  it("shows the error when the check fails and allows trying again", async () => {
    const invoke = vi
      .fn()
      .mockRejectedValueOnce("Could not check for updates: offline")
      .mockResolvedValueOnce(null);
    const service = create(invoke);
    await service.check();
    expect(service.state()).toBe("error");
    expect(service.error()).toContain("offline");
    await service.check();
    expect(service.state()).toBe("upToDate");
    expect(service.error()).toBeNull();
  });

  it("ignores a second check while one is running", async () => {
    let resolve: (value: UpdateInfo | null) => void = () => {};
    const invoke = vi.fn(() => new Promise<UpdateInfo | null>((r) => (resolve = r)));
    const service = create(invoke);
    const first = service.check();
    await service.check();
    expect(invoke).toHaveBeenCalledTimes(1);
    resolve(null);
    await first;
  });

  it("installs only after an update was found", async () => {
    const invoke = vi.fn(async (command: string) => (command === "update_check" ? newer : null));
    const service = create(invoke);
    await service.install();
    expect(invoke).not.toHaveBeenCalled();
    await service.check();
    await service.install();
    expect(invoke).toHaveBeenLastCalledWith("update_install");
    expect(service.state()).toBe("installing");
  });

  it("returns to the error state when the installation fails", async () => {
    const service = create(async (command) => {
      if (command === "update_install") throw "Could not install the update: disk full";
      return newer;
    });
    await service.check();
    await service.install();
    expect(service.state()).toBe("error");
    expect(service.error()).toContain("disk full");
  });
});

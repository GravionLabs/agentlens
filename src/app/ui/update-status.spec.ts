// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc } from "../core/ipc";
import type { UpdateInfo } from "../core/types";
import { UpdateService } from "../core/update.service";
import { UpdateStatus } from "./update-status";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

const newer: UpdateInfo = { version: "0.5.0", currentVersion: "0.4.0", notes: "", date: null };

function render(invoke: (command: string) => Promise<unknown>) {
  TestBed.configureTestingModule({ providers: [{ provide: Ipc, useValue: { invoke } }] });
  const fixture = TestBed.createComponent(UpdateStatus);
  const service = TestBed.inject(UpdateService);
  const text = () => (fixture.nativeElement as HTMLElement).textContent ?? "";
  const buttons = () =>
    Array.from((fixture.nativeElement as HTMLElement).querySelectorAll("button")).map((b) =>
      (b.textContent ?? "").trim(),
    );
  return { fixture, service, text, buttons };
}

describe("UpdateStatus", () => {
  it("shows a failed check with its error as visible text", async () => {
    const { fixture, service, text } = render(async () => {
      throw "Could not check for updates: offline";
    });
    await service.check();
    fixture.detectChanges();
    expect(text()).toContain("Update check failed");
    expect(text()).toContain("offline");
  });

  it("shows a failed installation with its error and still offers the installation", async () => {
    const { fixture, service, text, buttons } = render(async (command) => {
      if (command === "update_install") throw "Could not install the update: no package";
      return newer;
    });
    await service.check();
    await service.install();
    fixture.detectChanges();
    expect(text()).toContain("Update failed");
    expect(text()).not.toContain("Update check failed");
    expect(text()).toContain("no package");
    expect(buttons()).toContain("Install and restart");
  });
});

// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
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

  it("offers a new check but no installation after a failed check", async () => {
    const { fixture, service, buttons } = render(async () => {
      throw "offline";
    });
    await service.check();
    fixture.detectChanges();
    expect(buttons()).toEqual(["Check for updates"]);
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

  it("offers only the installation, not a new check, after a failed installation", async () => {
    const { fixture, service, buttons } = render(async (command) => {
      if (command === "update_install") throw "no package";
      return newer;
    });
    await service.check();
    await service.install();
    fixture.detectChanges();
    expect(buttons()).toEqual(["Install and restart"]);
  });

  it("retries the installation when the button is clicked", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "update_install") throw "no package";
      return newer;
    });
    const { fixture, service } = render(invoke);
    await service.check();
    await service.install();
    fixture.detectChanges();
    const button = (fixture.nativeElement as HTMLElement).querySelector("button");
    button?.click();
    expect(invoke.mock.calls.filter(([c]) => c === "update_install")).toHaveLength(2);
  });
});

describe("UpdateStatus messages", () => {
  it("renders a failed check as an alert and an up to date result as a status", async () => {
    const { fixture, service } = render(async () => {
      throw "Could not check for updates: offline";
    });
    await service.check();
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('hx-message [role="alert"], hx-message[role="alert"]')).not.toBeNull();
  });
});

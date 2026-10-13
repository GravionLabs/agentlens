// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc, SAMPLE_MODE } from "../core/ipc";
import { TokenFormatService } from "../core/token-format.service";
import { tokens } from "../core/format";
import { StatusBar } from "./status-bar";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => {
  TestBed.inject(TokenFormatService).set("full");
  TestBed.resetTestingModule();
});

describe("StatusBar token format switch", () => {
  it("toggles the format, aria-pressed and the formatted values", async () => {
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useValue: { invoke: async () => "0.1.0" } }],
    });
    const fixture = TestBed.createComponent(StatusBar);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const button = el.querySelector<HTMLButtonElement>('button[aria-label="Round token counts"]')!;
    expect(button.getAttribute("aria-pressed")).toBe("false");
    expect(tokens(2114305)).not.toContain("M");
    button.click();
    fixture.detectChanges();
    expect(button.getAttribute("aria-pressed")).toBe("true");
    expect(button.title).toContain("full");
    expect(tokens(2114305)).toContain("M");
    button.click();
    fixture.detectChanges();
    expect(button.getAttribute("aria-pressed")).toBe("false");
    expect(tokens(2114305)).not.toContain("M");
  });
});

describe("StatusBar command line tools", () => {
  it("has no command line tools in sample mode", async () => {
    TestBed.configureTestingModule({
      providers: [
        { provide: Ipc, useValue: { invoke: async () => "0.1.0" } },
        { provide: SAMPLE_MODE, useValue: true },
      ],
    });
    const fixture = TestBed.createComponent(StatusBar);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector("app-cli-tools")).toBeNull();
    expect(el.textContent).toContain("sample data");
  });

  it("has the command line tools outside sample mode", async () => {
    TestBed.configureTestingModule({
      providers: [
        { provide: Ipc, useValue: { invoke: async () => "0.1.0" } },
        { provide: SAMPLE_MODE, useValue: false },
      ],
    });
    const fixture = TestBed.createComponent(StatusBar);
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector("app-cli-tools")).not.toBeNull();
  });
});

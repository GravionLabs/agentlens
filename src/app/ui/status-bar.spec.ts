// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc, SAMPLE_MODE } from "../core/ipc";
import { TokenFormatService } from "../core/token-format.service";
import { tokens } from "../core/format";
import { ZoomService } from "../core/zoom.service";
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

async function renderBar(sample: boolean, version: string | null) {
  TestBed.configureTestingModule({
    providers: [
      {
        provide: Ipc,
        useValue: {
          invoke: async () => {
            if (version === null) throw "no version";
            return version;
          },
        },
      },
      { provide: SAMPLE_MODE, useValue: sample },
    ],
  });
  const fixture = TestBed.createComponent(StatusBar);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  return { el, text: (el.textContent ?? "").replace(/\s+/g, " ") };
}

describe("StatusBar sample mode", () => {
  it("labels sample mode as sample data with the install link", async () => {
    const { el } = await renderBar(true, "0.7.1");
    expect(el.querySelector("hx-tag.sample")?.textContent?.trim()).toBe("sample data");
    const link = el.querySelector<HTMLAnchorElement>("a.demo-link")!;
    expect(link.textContent?.trim()).toBe("Install the application");
    expect(link.href).toContain("releases/latest");
  });

  it("has no sample badge outside sample mode", async () => {
    const { el } = await renderBar(false, "0.7.1");
    expect(el.querySelector(".sample")).toBeNull();
    expect(el.querySelector(".demo-link")).toBeNull();
  });

  it("keeps the token toggle and the zoom controls in sample mode", async () => {
    const { el } = await renderBar(true, null);
    for (const label of ["Round token counts", "Smaller", "Reset size", "Larger"]) {
      expect(el.querySelector(`button[aria-label="${label}"]`), label).not.toBeNull();
    }
  });
});

describe("StatusBar zoom controls", () => {
  it("steps the size and shows the percentage on helix buttons", async () => {
    localStorage.clear();
    const { el } = await renderBar(false, "0.7.1");
    const zoom = TestBed.inject(ZoomService);
    const click = (label: string) =>
      el.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!.click();
    const reset = el.querySelector<HTMLButtonElement>('button[aria-label="Reset size"]')!;
    expect(reset.classList.contains("hx-button")).toBe(true);
    expect(reset.textContent?.trim()).toBe("100%");
    click("Larger");
    expect(zoom.factor()).toBe(1.1);
    click("Smaller");
    click("Smaller");
    expect(zoom.factor()).toBe(0.9);
    click("Reset size");
    expect(zoom.factor()).toBe(1);
  });

  it("stays within 70 to 160 percent", async () => {
    localStorage.clear();
    const { el } = await renderBar(false, "0.7.1");
    const zoom = TestBed.inject(ZoomService);
    for (let i = 0; i < 12; i++)
      el.querySelector<HTMLButtonElement>('[aria-label="Smaller"]')!.click();
    expect(zoom.factor()).toBe(0.7);
    for (let i = 0; i < 12; i++)
      el.querySelector<HTMLButtonElement>('[aria-label="Larger"]')!.click();
    expect(zoom.factor()).toBe(1.6);
    zoom.reset();
  });
});

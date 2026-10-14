// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { ExportService } from "../core/export.service";
import { SAMPLE_MODE } from "../core/ipc";
import { ExportMenu } from "./export-menu";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => {
  TestBed.resetTestingModule();
  document.querySelectorAll(".cdk-overlay-container").forEach((e) => e.remove());
});

function create(opts: { sample?: boolean; formats?: string[] } = {}) {
  const exportFn = vi.fn(async () => {});
  TestBed.configureTestingModule({
    providers: [
      { provide: ExportService, useValue: { status: () => ({ kind: "idle" }), export: exportFn } },
      { provide: SAMPLE_MODE, useValue: opts.sample ?? false },
    ],
  });
  const fixture = TestBed.createComponent(ExportMenu);
  // The vitest setup compiles in JIT mode, which cannot bind signal inputs: replace them.
  Object.assign(fixture.componentInstance, {
    view: () => "cost",
    formats: () => opts.formats ?? ["json", "html", "csv"],
    options: () => ({ by: "project" }),
  });
  fixture.detectChanges();
  const trigger = fixture.nativeElement.querySelector("button") as HTMLButtonElement;
  const items = () => Array.from(document.querySelectorAll<HTMLElement>("[role=menuitem]"));
  const settle = async () => {
    await fixture.whenStable();
    fixture.detectChanges();
  };
  return { fixture, trigger, items, settle, exportFn };
}

describe("ExportMenu", () => {
  it("lists the formats of the view when the trigger is activated", async () => {
    const { trigger, items, settle } = create();
    expect(items()).toHaveLength(0);
    trigger.click();
    await settle();
    expect(items().map((i) => i.textContent?.trim())).toEqual(["JSON", "HTML report", "CSV"]);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
  });

  it("exports the chosen format with the options of the view and closes", async () => {
    const { trigger, items, settle, exportFn } = create();
    trigger.click();
    await settle();
    items()[1].click();
    await settle();
    expect(exportFn).toHaveBeenCalledWith("cost", "html", { by: "project" });
    expect(items()).toHaveLength(0);
  });

  it("moves with the arrow keys and closes with Escape", async () => {
    const { trigger, items, settle, exportFn } = create();
    trigger.click();
    await settle();
    // The CDK menu reads keyCode (40 = ArrowDown, 27 = Escape).
    const key = (el: Element, k: string, keyCode: number) =>
      el.dispatchEvent(new KeyboardEvent("keydown", { key: k, keyCode, bubbles: true }));
    items()[0].focus();
    key(items()[0], "ArrowDown", 40);
    await settle();
    expect(document.activeElement).toBe(items()[1]);
    key(items()[1], "Escape", 27);
    await settle();
    expect(items()).toHaveLength(0);
    expect(exportFn).not.toHaveBeenCalled();
  });

  it("uses real buttons, so Tab reaches the trigger and Enter activates an item", async () => {
    const { trigger, items, settle, exportFn } = create();
    expect(trigger.tagName).toBe("BUTTON");
    expect(trigger.tabIndex).toBe(0);
    trigger.click();
    await settle();
    // Enter on a focused button is a click in the browser; jsdom does not synthesize it.
    expect(items().every((i) => i.tagName === "BUTTON")).toBe(true);
    items()[2].click();
    await settle();
    expect(exportFn).toHaveBeenCalledWith("cost", "csv", { by: "project" });
  });

  it("is disabled in sample mode and offers no menu", async () => {
    const { trigger, items, settle } = create({ sample: true });
    expect(trigger.disabled).toBe(true);
    expect(trigger.title).toContain("sample data");
    trigger.click();
    await settle();
    expect(items()).toHaveLength(0);
  });
});

// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { Status } from "./status";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

function create(
  values: Partial<{ loading: boolean; error: unknown; warnings: number; errors: string[] }>,
) {
  const fixture = TestBed.createComponent(Status);
  // The vitest setup compiles in JIT mode, which cannot bind signal inputs: replace them.
  Object.assign(fixture.componentInstance, {
    loading: () => values.loading ?? false,
    error: () => values.error,
    warnings: () => values.warnings ?? 0,
    errors: () => values.errors ?? [],
  });
  fixture.detectChanges();
  const retry = vi.fn();
  fixture.componentInstance.retry.subscribe(retry);
  return Object.assign(fixture.nativeElement as HTMLElement, { retry });
}

describe("Status", () => {
  it("shows nothing when there is nothing to report", () => {
    expect(create({}).querySelector("hx-message")).toBeNull();
  });

  it("shows the loading message with a spinner", () => {
    const el = create({ loading: true });
    expect(el.textContent).toContain("Reading sessions…");
    expect(el.querySelector("hx-message hx-progress-spinner")).not.toBeNull();
    expect(el.querySelector("hx-message")?.getAttribute("role")).toBe("status");
  });

  it("shows a failure as an alert with its message", () => {
    const el = create({ error: "boom" });
    const message = el.querySelector("hx-message.hx-message-error")!;
    expect(message.getAttribute("role")).toBe("alert");
    expect(message.textContent).toContain("boom");
  });

  it("shows the number of warnings only when there are some", () => {
    expect(create({ warnings: 0 }).textContent).not.toContain("warnings");
    expect(create({ warnings: 3 }).textContent).toContain("3 warnings while reading sessions.");
  });

  it("lists each error of the analysis", () => {
    const el = create({ errors: ["first", "second"] });
    const texts = Array.from(el.querySelectorAll("hx-message")).map((m) => m.textContent?.trim());
    expect(texts).toEqual(["first", "second"]);
  });

  it("offers to try again inside the failure message", () => {
    const el = create({ error: "boom" });
    // A native button: Tab reaches it, Enter and Space press it.
    const button = el.querySelector<HTMLButtonElement>("hx-message.hx-message-error button")!;
    expect(button.textContent!.trim()).toBe("Try again");
    expect(button.type).toBe("button");
    expect(el.retry).not.toHaveBeenCalled();
    button.click();
    expect(el.retry).toHaveBeenCalledOnce();
  });

  it("has no button without a failure", () => {
    const el = create({ loading: true, warnings: 2, errors: ["unreadable file"] });
    expect(el.querySelector("button")).toBeNull();
  });
});

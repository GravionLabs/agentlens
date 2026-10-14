// @vitest-environment jsdom
import { NO_ERRORS_SCHEMA } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { HxDataTable, HxTableBody } from "@gravionlabs/helix-ui";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { Ipc } from "../../core/ipc";
import { tokenFormat } from "../../core/token-format.service";
import type { Answer, CompareReport } from "../../core/types";
import samples from "../../core/samples/compare.json";
import { Compare, METRICS } from "./compare";

describe("compare metrics", () => {
  const sources = (samples as unknown as Answer<CompareReport>).data.sources;

  it("show values a client does not record as n/a, never as 0", () => {
    const claude = sources.find((s) => s.source === "claude-code")!;
    const rate = METRICS.find((m) => m.label === "Tool error rate")!;
    expect(rate.value({ ...claude, tool_error_rate: null })).toBe("n/a");
    const requests = METRICS.find((m) => m.label === "Requests")!;
    expect(requests.value({ ...claude, requests: null })).toBe("n/a");
    expect(requests.value({ ...claude, requests: 3 })).toBe("3");
  });

  it("mark the skill overhead as an estimate", () => {
    const m = METRICS.find((x) => x.label === "Skill overhead")!;
    expect(m.estimated).toBe(true);
    expect(
      m.value({ ...sources[0], skill_overhead_tokens: { count: 1234, measured: false } }),
    ).toBe("~1,234");
    expect(m.value({ ...sources[0], skill_overhead_tokens: null })).toBe("n/a");
    expect(m.value({ ...sources[0], skill_overhead_tokens: { count: 5, measured: true } })).toBe(
      "5",
    );
  });
});

describe("compare token metrics", () => {
  const s = (samples as unknown as Answer<CompareReport>).data.sources[0];
  afterEach(() => tokenFormat.set("full"));

  it("round with the setting and expose the exact count", () => {
    const m = METRICS.find((x) => x.label === "Input tokens")!;
    expect(m.value({ ...s, input_tokens: 2_114_305 })).toBe("2,114,305");
    tokenFormat.set("rounded");
    expect(m.value({ ...s, input_tokens: 2_114_305 })).toBe("2.11 M");
    expect(m.exact?.({ ...s, input_tokens: 2_114_305 })).toBe(2_114_305);
    expect(m.value({ ...s, input_tokens: null })).toBe("n/a");
  });
});

let payload: unknown = null;

class FakeIpc extends Ipc {
  override async invoke<T>(): Promise<T> {
    return { schema_version: 2, data: payload, warnings: 0, errors: [] } as T;
  }
}

describe("Compare table on helix", () => {
  const base = samples as unknown as Answer<CompareReport>;
  beforeAll(() => {
    TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
  });
  beforeEach(() => {
    tokenFormat.set("rounded");
    TestBed.configureTestingModule({ providers: [{ provide: Ipc, useClass: FakeIpc }] });
  });
  afterEach(() => TestBed.resetTestingModule());

  async function render(sources: unknown[]) {
    payload = { sources };
    TestBed.overrideComponent(Compare, {
      set: { imports: [HxDataTable, HxTableBody], schemas: [NO_ERRORS_SCHEMA] },
    });
    const fixture = TestBed.createComponent(Compare);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    return fixture.nativeElement as HTMLElement;
  }

  const row = (el: HTMLElement, label: string) =>
    [...el.querySelectorAll("hx-data-table tbody tr")].find(
      (r) => r.querySelector("th")?.textContent?.trim() === label,
    )!;

  it("has one column per client and one row per metric", async () => {
    const el = await render([
      { ...base.data.sources[0], source: "claude-code", requests: 3 },
      { ...base.data.sources[0], source: "opencode", requests: null },
    ]);
    const heads = [...el.querySelectorAll("hx-data-table thead th")].map((h) =>
      h.textContent!.trim(),
    );
    expect(heads).toEqual(["Metric", "Claude Code", "OpenCode"]);
    expect(el.querySelectorAll("hx-data-table tbody tr")).toHaveLength(METRICS.length);
    const cells = [...row(el, "Requests").querySelectorAll("td")].map((c) => c.textContent!.trim());
    expect(cells).toEqual(["3", "n/a"]);
  });

  it("marks estimates but not n/a, and keeps the exact token count as title", async () => {
    const el = await render([
      {
        ...base.data.sources[0],
        input_tokens: 2_114_305,
        skill_overhead_tokens: { count: 10, measured: false },
      },
      { ...base.data.sources[0], source: "opencode", skill_overhead_tokens: null },
    ]);
    const [est, none] = row(el, "Skill overhead").querySelectorAll("td");
    expect(est.classList.contains("est")).toBe(true);
    expect(est.textContent!.trim()).toBe("~10");
    expect(none.classList.contains("est")).toBe(false);
    expect(none.textContent!.trim()).toBe("n/a");
    const input = row(el, "Input tokens").querySelector("td")!;
    expect(input.textContent!.trim()).toBe("2.11 M");
    expect(input.getAttribute("title")).toBe("2,114,305");
  });
});

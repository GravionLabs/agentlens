// @vitest-environment jsdom
import { NO_ERRORS_SCHEMA } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
import { HxCell, HxDataTable } from "@gravionlabs/helix-ui";
import { Ipc } from "../../core/ipc";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { tokenFormat } from "../../core/token-format.service";
import type { Finding } from "../../core/types";
import { Advice, findingTitle, findingTotal, itemView } from "./advice";

const finding = (over: Partial<Finding>): Finding => ({
  kind: "unused-skills",
  explanation: "",
  action: "",
  count: 2,
  total: { count: 12_235_734, measured: false },
  share_of_baseline: 0.3014,
  share_of_total_tokens: null,
  items: [],
  notes: [],
  ...over,
});

describe("findings", () => {
  it("have a title for every kind", () => {
    expect(findingTitle("unused-skills")).toBe("Unused skills");
    expect(findingTitle("large-context")).toBe("Work on a large context");
    expect(findingTitle("listed-but-loaded-by-hand")).toBe("Listed but loaded by hand");
    expect(findingTitle("costly-mcp-servers")).toBe("Costly MCP servers");
    expect(findingTitle("context-size")).toBe("Context size");
  });

  it("say what the sum is, and mark an estimate", () => {
    expect(findingTotal(finding({}))).toBe(
      "Together they cost ~12,235,734 tokens, 30% of the baseline.",
    );
    expect(findingTotal(finding({ kind: "costly-descriptions", share_of_baseline: null }))).toBe(
      "Together they could save ~12,235,734 tokens.",
    );
    expect(
      findingTotal(finding({ total: { count: 5, measured: true }, share_of_baseline: null })),
    ).toBe("Together they cost 5 tokens.");
    expect(findingTotal(finding({ share_of_total_tokens: 0.0035 }))).toBe(
      "Together they cost ~12,235,734 tokens, 30% of the baseline, ~0.35% of all tokens.",
    );
    expect(
      findingTotal(
        finding({
          total: { count: 9, measured: true },
          share_of_baseline: null,
          share_of_total_tokens: 0,
        }),
      ),
    ).toBe("Together they cost 9 tokens, 0.00% of all tokens.");
    expect(findingTotal(finding({ total: null }))).toBeNull();
  });
});

describe("advice totals in rounded mode", () => {
  afterEach(() => tokenFormat.set("full"));

  it("rounds the sentence", () => {
    tokenFormat.set("rounded");
    expect(findingTotal(finding({ share_of_baseline: null }))).toBe(
      "Together they cost ~12.24 M tokens.",
    );
  });
});

let payload: unknown = null;

class FakeIpc extends Ipc {
  override async invoke<T>(): Promise<T> {
    return { schema_version: 2, data: payload, warnings: 0, errors: [] } as T;
  }
}

async function render() {
  TestBed.overrideComponent(Advice, {
    set: { imports: [HxDataTable, HxCell], schemas: [NO_ERRORS_SCHEMA] },
  });
  const fixture = TestBed.createComponent(Advice);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement };
}

const column = (el: HTMLElement, index: number) =>
  [...el.querySelectorAll("hx-data-table tbody tr")].map((r) =>
    r.querySelectorAll("td")[index].textContent!.trim(),
  );

const headerButton = (el: HTMLElement, name: string) =>
  [...el.querySelectorAll<HTMLElement>("hx-data-table th button")].find((b) =>
    b.textContent!.includes(name),
  )!;

describe("Advice table on helix", () => {
  beforeAll(() => {
    TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
  });
  beforeEach(() => {
    tokenFormat.set("full");
    payload = {
      sessions: 3,
      installed_skills: 4,
      baseline_total: { count: 10, measured: true },
      findings: [
        finding({
          items: [
            { name: "alpha", tokens: { count: 100, measured: true }, requests: 1, detail: "d1" },
            { name: "beta", tokens: { count: 900, measured: false }, requests: 12, detail: "d2" },
          ],
        }),
      ],
    };
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
  });
  afterEach(() => TestBed.resetTestingModule());

  it("shows headers and the rows of a finding", async () => {
    const { el } = await render();
    const heads = [...el.querySelectorAll("hx-data-table th")].map((h) => h.textContent!.trim());
    expect(heads.join("|")).toContain("Skill");
    expect(heads.join("|")).toContain("Tokens");
    expect(column(el, 0)).toEqual(["alpha", "beta"]);
    expect(column(el, 2)).toEqual(["1", "12"]);
  });

  it("marks an estimated token count with a tilde and the estimate colour", async () => {
    const { el } = await render();
    expect(column(el, 1)).toEqual(["100", "~900"]);
    const spans = el.querySelectorAll("hx-data-table tbody span.est");
    expect(spans).toHaveLength(1);
    expect(spans[0].textContent).toContain("~900");
  });

  it("sorts by a header click, descending after ascending", async () => {
    const { fixture, el } = await render();
    headerButton(el, "Tokens").click();
    fixture.detectChanges();
    expect(column(el, 0)).toEqual(["alpha", "beta"]);
    headerButton(el, "Tokens").click();
    fixture.detectChanges();
    expect(column(el, 0)).toEqual(["beta", "alpha"]);
  });

  it("keeps the raw count for sorting next to the text", () => {
    const v = itemView({
      name: "x",
      tokens: { count: 5, measured: false },
      requests: 1,
      detail: "",
    });
    expect(v).toMatchObject({ tokens: 5, tokensText: "~5", estimated: true });
  });
});

// @vitest-environment jsdom
import { NO_ERRORS_SCHEMA } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
import { HxCell, HxColumnHeader, HxDataTable } from "@gravionlabs/helix-ui";
import { HelpLinkStub } from "../../ui/help-link.testing";
import { Ipc } from "../../core/ipc";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { tokenFormat } from "../../core/token-format.service";
import type { Finding, Tip, TipsReport } from "../../core/types";
import {
  Advice,
  findingSaving,
  findingTitle,
  findingTotal,
  itemView,
  noTipsText,
  stepText,
  tipSaving,
} from "./advice";

const finding = (over: Partial<Finding>): Finding => ({
  rank: 1,
  kind: "unused-skills",
  explanation: "",
  action: "",
  count: 2,
  total: { count: 12_235_734, measured: false },
  share_of_baseline: 0.3014,
  share_of_total_tokens: null,
  items: [],
  notes: [],
  impact: {
    saving: { count: 12_235_734, measured: false },
    saving_usd: 1.5,
    confidence: "estimated",
  },
  ...over,
});

describe("findings", () => {
  it("have a title for every kind", () => {
    expect(findingTitle("unused-skills")).toBe("Unused skills");
    expect(findingTitle("large-context")).toBe("Work on a large context");
    expect(findingTitle("listed-but-loaded-by-hand")).toBe("Listed but loaded by hand");
    expect(findingTitle("costly-mcp-servers")).toBe("Costly MCP servers");
    expect(findingTitle("context-size")).toBe("Context size");
    expect(findingTitle("context-hygiene")).toBe("Context hygiene");
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

describe("finding savings", () => {
  it("give the USD saving with a tilde, the tokens and the confidence", () => {
    expect(findingSaving(finding({}))).toBe("Could save ~$1.50 (~12,235,734 tokens, estimated).");
    expect(
      findingSaving(
        finding({
          impact: {
            saving: { count: 400, measured: true },
            saving_usd: 0.004,
            confidence: "measured",
          },
        }),
      ),
    ).toBe("Could save ~$0.00 (400 tokens, measured).");
  });

  it("say when no price is known, and nothing without a saving", () => {
    expect(
      findingSaving(
        finding({
          impact: {
            saving: { count: 9, measured: false },
            saving_usd: null,
            confidence: "inferred",
          },
        }),
      ),
    ).toBe("Could save ~9 tokens (inferred; no price known).");
    expect(
      findingSaving(
        finding({
          impact: {
            saving: { count: 0, measured: true },
            saving_usd: null,
            confidence: "measured",
          },
        }),
      ),
    ).toBeNull();
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
let tipsPayload: TipsReport = {
  period_cost_usd: null,
  threshold: 0.005,
  tips: [],
  below_threshold: 0,
};

class FakeIpc extends Ipc {
  override async invoke<T>(command: string): Promise<T> {
    const data = command === "tips" ? tipsPayload : payload;
    return { schema_version: 2, data, warnings: 0, errors: [] } as T;
  }
}

const tip = (over: Partial<Tip> = {}): Tip => ({
  rank: 1,
  kind: "unused-skills",
  headline: "Remove 2 unused skills",
  saving: { count: 1000, measured: false },
  saving_usd: 1.234,
  share: 0.0456,
  confidence: "estimated",
  action: "Delete the skill folders.",
  targets: [
    {
      name: "alpha",
      detail: "never used",
      steps: [
        { client: "Claude Code", file: ".claude/settings.json", change: '{"a":1}' },
        { client: "OpenCode", file: null, change: "remove alpha" },
      ],
    },
  ],
  more: 3,
  ...over,
});

async function render() {
  TestBed.overrideComponent(Advice, {
    set: {
      imports: [HxDataTable, HxCell, HxColumnHeader, HelpLinkStub],
      schemas: [NO_ERRORS_SCHEMA],
    },
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

  it("heads the context size table Session and keeps the full id in a tooltip", async () => {
    payload.findings.push(
      finding({
        kind: "context-size",
        items: [
          {
            name: "shop · cad3c7a2",
            session_id: "cad3c7a2-f626-4332-b1af-29291ebd5c7b",
            tokens: { count: 5, measured: true },
            requests: 2,
            detail: "d",
          },
        ],
      }),
    );
    const { el } = await render();
    const tables = [...el.querySelectorAll("hx-data-table")];
    const head = (t: Element) => t.querySelector("th")!.textContent!.trim();
    expect(head(tables[0])).toBe("Skill");
    expect(head(tables[1])).toBe("Session");
    const cell = tables[1].querySelector("tbody span[title]")!;
    expect(cell.textContent).toContain("shop · cad3c7a2");
    expect(cell.getAttribute("title")).toBe("cad3c7a2-f626-4332-b1af-29291ebd5c7b");
  });

  it("heads the context hygiene table Session too", async () => {
    payload.findings.push(
      finding({
        kind: "context-hygiene",
        items: [
          {
            name: "shop · cad3c7a2",
            session_id: "cad3c7a2-f626-4332-b1af-29291ebd5c7b",
            tokens: { count: 5, measured: true },
            requests: 2,
            detail: "2 requests, peak 6,000 tokens, 2 compactions, ~1 clear, not near the limit",
          },
        ],
      }),
    );
    const { el } = await render();
    const tables = [...el.querySelectorAll("hx-data-table")];
    expect(tables[1].querySelector("th")!.textContent!.trim()).toBe("Session");
    const cell = tables[1].querySelector("tbody span[title]")!;
    expect(cell.getAttribute("title")).toBe("cad3c7a2-f626-4332-b1af-29291ebd5c7b");
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

  it("keeps the server order and shows rank and saving", async () => {
    payload = {
      ...(payload as object),
      findings: [
        finding({ rank: 1, kind: "context-size" }),
        finding({
          rank: 2,
          kind: "unused-skills",
          impact: { saving: { count: 5, measured: true }, saving_usd: 9, confidence: "measured" },
        }),
        finding({
          rank: 3,
          kind: "large-context",
          impact: {
            saving: { count: 0, measured: true },
            saving_usd: null,
            confidence: "measured",
          },
        }),
      ],
    };
    const { el } = await render();
    const titles = [...el.querySelectorAll(".finding h3")].map((h) =>
      h.textContent!.replace(/\s+/g, " ").trim(),
    );
    expect(titles).toEqual([
      "1. Context size (2)",
      "2. Unused skills (2)",
      "3. Work on a large context (2)",
    ]);
    const savings = [...el.querySelectorAll(".saving")].map((p) => p.textContent!.trim());
    expect(savings).toEqual([
      "Could save ~$1.50 (~12,235,734 tokens, estimated).",
      "Could save ~$9.00 (5 tokens, measured).",
    ]);
  });

  it("links the Tokens and Requests headers to their glossary entries", async () => {
    const { el } = await render();
    const links = [...el.querySelectorAll<HTMLAnchorElement>("hx-data-table th a.help")];
    expect(links.map((a) => a.getAttribute("aria-label"))).toEqual([
      "Explain Tokens",
      "Explain Requests",
    ]);
    expect(links[0].getAttribute("href")).toBe("/glossary?term=token");
  });
});

describe("tip helpers", () => {
  it("show the saving with a tilde and the share, or the tokens without prices", () => {
    expect(tipSaving(tip())).toBe("~$1.23 (4.6%)");
    expect(tipSaving(tip({ share: null }))).toBe("~$1.23");
    expect(tipSaving(tip({ saving_usd: null }))).toBe("~1,000 tokens");
    expect(tipSaving(tip({ saving_usd: null, saving: { count: 5, measured: true } }))).toBe(
      "5 tokens",
    );
  });

  it("copy file, newline and change, or the change alone", () => {
    expect(stepText({ client: "c", file: "a.json", change: "x" })).toBe("a.json\nx");
    expect(stepText({ client: "c", file: null, change: "x" })).toBe("x");
  });

  it("explain an empty list with the threshold", () => {
    expect(noTipsText(0.005)).toBe("No change would save more than 0.5% of the period's cost.");
  });
});

describe("Advice tips", () => {
  let written: string[];
  let fail: boolean;
  beforeAll(() => {
    try {
      TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
    } catch {
      // already initialised by the other suite
    }
  });
  beforeEach(() => {
    written = [];
    fail = false;
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: (t: string) => {
          if (fail) return Promise.reject(new Error("denied"));
          written.push(t);
          return Promise.resolve();
        },
      },
    });
    payload = {
      sessions: 1,
      installed_skills: 1,
      baseline_total: { count: 1, measured: true },
      findings: [finding({ kind: "unused-skills" })],
    };
    tipsPayload = {
      period_cost_usd: 50,
      threshold: 0.005,
      tips: [tip(), tip({ rank: 2, kind: "large-context", headline: "Second", more: 0 })],
      below_threshold: 4,
    };
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
  });
  afterEach(() => {
    vi.useRealTimers();
    TestBed.resetTestingModule();
  });

  it("renders one card per tip in rank order with saving, confidence and more", async () => {
    const { el } = await render();
    const cards = [...el.querySelectorAll<HTMLElement>(".tip")];
    expect(
      cards.map((c) => c.querySelector("h4")!.textContent!.replace(/\s+/g, " ").trim()),
    ).toEqual(["#1 Remove 2 unused skills", "#2 Second"]);
    expect(cards[0].querySelector(".saving strong")!.textContent!.trim()).toBe("~$1.23 (4.6%)");
    expect(cards[0].querySelector(".confidence")!.textContent).toContain("estimated");
    expect(cards[0].querySelector("a.help")!.getAttribute("href")).toBe(
      "/glossary?term=confidence",
    );
    expect(cards[0].textContent).toContain("Delete the skill folders.");
    expect(cards[0].textContent).toContain("alpha");
    expect(cards[0].textContent).toContain("+3 more");
    expect(cards[1].textContent).not.toContain("more\n");
    expect(cards[1].querySelector(".muted:not(.none)")?.textContent ?? "").not.toContain("+");
    expect(el.querySelector(".below")!.textContent!.replace(/\s+/g, " ")).toContain(
      "4 more findings save less than 0.5% each; they are listed below.",
    );
  });

  it("copies file and change, then shows Copied for two seconds", async () => {
    const { fixture, el } = await render();
    vi.useFakeTimers();
    const buttons = el.querySelectorAll<HTMLButtonElement>(".tip .copy");
    buttons[0].click();
    await vi.advanceTimersByTimeAsync(0);
    fixture.detectChanges();
    expect(written).toEqual(['.claude/settings.json\n{"a":1}']);
    expect(buttons[0].textContent!.trim()).toBe("Copied");
    expect(buttons[1].textContent!.trim()).toBe("Copy");
    await vi.advanceTimersByTimeAsync(2000);
    fixture.detectChanges();
    expect(buttons[0].textContent!.trim()).toBe("Copy");
    buttons[1].click();
    await vi.advanceTimersByTimeAsync(0);
    expect(written[1]).toBe("remove alpha");
  });

  it("shows the text selected when the clipboard write fails", async () => {
    fail = true;
    const { fixture, el } = await render();
    el.querySelector<HTMLButtonElement>(".tip .copy")!.click();
    await fixture.whenStable();
    fixture.detectChanges();
    await fixture.whenStable();
    const box = el.querySelector<HTMLTextAreaElement>(".manual")!;
    expect(box.value).toBe('.claude/settings.json\n{"a":1}');
    expect(el.querySelector(".tip .copy")!.textContent!.trim()).toBe("Copy");
  });

  it("scrolls to the finding of the tip", async () => {
    const { el } = await render();
    const target = el.querySelector<HTMLElement>("#finding-unused-skills")!;
    expect(target).not.toBeNull();
    const scroll = vi.fn();
    target.scrollIntoView = scroll;
    document.body.appendChild(el);
    el.querySelector<HTMLAnchorElement>(".tip a.see")!.click();
    expect(scroll).toHaveBeenCalled();
    expect(el.querySelector(".tip a.see")!.getAttribute("href")).toBe("#finding-unused-skills");
    el.remove();
  });

  it("says so when no change saves enough", async () => {
    tipsPayload = { ...tipsPayload, tips: [], below_threshold: 0 };
    const { el } = await render();
    expect(el.querySelector(".none")!.textContent!.trim()).toBe(
      "No change would save more than 0.5% of the period's cost.",
    );
    expect(el.querySelector(".below")).toBeNull();
    expect(el.querySelectorAll(".finding")).toHaveLength(1);
  });
});

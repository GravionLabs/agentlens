// @vitest-environment jsdom
import { NO_ERRORS_SCHEMA } from "@angular/core";
import {
  HxButton,
  HxCell,
  HxChip,
  HxDataTable,
  HxField,
  HxInput,
  HxSelect,
} from "@gravionlabs/helix-ui";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
import { beforeAll, beforeEach, afterEach, describe, expect, it } from "vitest";
import { Ipc } from "../../core/ipc";
import { tokenFormat } from "../../core/token-format.service";
import type { CostRow } from "../../core/types";
import type { CostData, WithoutReport } from "../../core/types";
import { WithoutService } from "../../core/without.service";
import { Cost, money, signedUsd, whatIfView, withoutViews } from "./cost";

const row = (over: Partial<CostRow>): CostRow => ({
  key: "m",
  requests: 2,
  input: 0,
  output: 0,
  cache_read: 0,
  cache_write: 0,
  measured_usd: 0,
  estimated_usd: 0,
  unpriced_requests: 0,
  premium_requests: 0,
  ...over,
});

describe("cost money", () => {
  it("marks computed values with a tilde and mixes both like the CLI", () => {
    expect(money(row({ estimated_usd: 1.5 }))).toBe("~$1.50");
    expect(money(row({ measured_usd: 2 }))).toBe("$2.00");
    expect(money(row({ measured_usd: 2, estimated_usd: 1.5 }))).toBe("$2.00 + ~$1.50");
  });

  it("says unpriced instead of showing zero when nothing has a price", () => {
    expect(money(row({ unpriced_requests: 2 }))).toBe("unpriced");
    expect(money(row({ requests: 0 }))).toBe("$0.00");
  });
});

describe("cost what-if", () => {
  it("computes the difference and signs it", () => {
    const v = whatIfView({
      model: "m",
      requests: 1,
      actual_usd: 3,
      what_if_usd: 1,
      excluded_requests: 0,
    });
    expect(v.diff).toBe(-2);
    expect(signedUsd(v.diff)).toBe("-$2.00");
    expect(signedUsd(1.5)).toBe("+$1.50");
    expect(signedUsd(-2, true)).toBe("-~$2.00");
  });

  it("has no prices and no difference for a row whose requests are all excluded", () => {
    const v = whatIfView({
      model: "m",
      requests: 0,
      actual_usd: 0,
      what_if_usd: 0,
      excluded_requests: 2,
    });
    expect([v.actual_usd, v.what_if_usd, v.diff]).toEqual([null, null, null]);
    expect(signedUsd(v.diff, true)).toBe("n/a");
    expect(v.excluded_requests).toBe(2);
  });
});

let rows: CostRow[] = [];
const baseRows = [
  row({ key: "model-a", requests: 1, input: 99999, output: 9999, measured_usd: 0.5 }),
  row({ key: "model-b", requests: 7, input: 10, output: 5, estimated_usd: 2 }),
  row({ key: "model-c", requests: 3, input: 20, output: 7, unpriced_requests: 3 }),
];

let withoutReport: WithoutReport | null = null;
let whatIfReport: CostData["what_if"] = null;
/** Keeps every request open, for the loading state. */
let pending = false;
let lastArgs: Record<string, unknown> = {};
let failure = "";

class FakeIpc extends Ipc {
  override async invoke<T>(_command: string, args?: Record<string, unknown>): Promise<T> {
    lastArgs = args ?? {};
    if (lastArgs["whatIf"] === "nope") throw "no price for the what-if model `nope`";
    if (failure) throw failure;
    if (pending) await new Promise(() => undefined);
    const data = {
      report: { rows, total: rows[0], unpriced_models: {} },
      what_if: whatIfReport,
      without: withoutReport,
    };
    return { schema_version: 2, data, warnings: 0, errors: [] } as T;
  }
}

describe("Cost table on helix", () => {
  beforeAll(() => {
    TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
  });
  beforeEach(() => {
    localStorage.clear();
    tokenFormat.set("rounded");
    rows = baseRows;
    withoutReport = null;
    whatIfReport = null;
    pending = false;
    lastArgs = {};
    failure = "";
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
  });
  afterEach(() => {
    TestBed.resetTestingModule();
    document.querySelectorAll(".cdk-overlay-container").forEach((e) => e.remove());
  });

  async function render(before: (w: WithoutService) => void = () => undefined) {
    // Signal inputs of app components need the Angular compiler plugin; only helix is rendered.
    TestBed.overrideComponent(Cost, {
      set: {
        imports: [HxDataTable, HxCell, HxField, HxSelect, HxInput, HxButton, HxChip],
        schemas: [NO_ERRORS_SCHEMA],
      },
    });
    before(TestBed.inject(WithoutService));
    const fixture = TestBed.createComponent(Cost);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    return { fixture, el: fixture.nativeElement as HTMLElement };
  }

  const firstColumn = (el: HTMLElement) =>
    [...el.querySelectorAll("hx-data-table tbody tr")].map((r) =>
      r.querySelector("td")!.textContent!.trim(),
    );

  it("renders the rows sorted by cost, descending, with n/a-free text and a tilde", async () => {
    const { el } = await render();
    expect(firstColumn(el)).toEqual(["model-b", "model-a", "model-c"]);
    const text = el.querySelector("hx-data-table tbody")!.textContent!;
    expect(text).toContain("~$2.00");
    expect(text).toContain("unpriced");
    expect(el.querySelector("th[aria-sort='descending']")!.textContent).toContain("Cost");
  });

  const headerButton = (el: HTMLElement, name: string) =>
    [...el.querySelectorAll<HTMLElement>("hx-data-table th button")].find((b) =>
      b.textContent!.includes(name),
    )!;

  it("sorts by a header click: ascending first, then descending", async () => {
    const { fixture, el } = await render();
    headerButton(el, "Requests").click();
    fixture.detectChanges();
    expect(firstColumn(el)).toEqual(["model-a", "model-c", "model-b"]);
    expect(el.querySelector("th[aria-sort='ascending']")!.textContent).toContain("Requests");
    headerButton(el, "Requests").click();
    fixture.detectChanges();
    expect(firstColumn(el)).toEqual(["model-b", "model-c", "model-a"]);
    expect(el.querySelector("th[aria-sort='descending']")!.textContent).toContain("Requests");
  });

  it("shows the premium requests column only when a row has premium requests", async () => {
    const without = await render();
    expect(without.el.querySelector("hx-data-table thead")!.textContent).not.toContain("Premium");
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
    rows = [...baseRows, row({ key: "model-d", premium_requests: 4 })];
    const withPremium = await render();
    expect(withPremium.el.querySelector("hx-data-table thead")!.textContent).toContain(
      "Premium requests",
    );
  });

  it("pages above 50 rows and shows everything on one page below", async () => {
    const small = await render();
    expect(firstColumn(small.el)).toHaveLength(3);
    expect(small.el.querySelector("hx-paginator")).toBeNull();
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
    rows = Array.from({ length: 51 }, (_, i) => row({ key: `m-${i}`, measured_usd: i + 1 }));
    const large = await render();
    expect(firstColumn(large.el)).toHaveLength(50);
    expect(large.el.querySelector("hx-paginator")).not.toBeNull();
  });

  it("puts the exact token count in the tooltip while counts are rounded", async () => {
    const { el } = await render();
    const titles = [...el.querySelectorAll("hx-data-table tbody span[title]")].map((s) =>
      s.getAttribute("title"),
    );
    expect(titles).toContain("99,999");
    expect(titles).toContain("model-a");
  });

  const report: WithoutReport = {
    rows: [
      {
        item: "demo",
        kind: "skill",
        installations: 2,
        requests: 4,
        tokens: { count: 1200, measured: false },
        cost_usd: 0.5,
        unpriced_requests: 0,
        detail: "",
      },
      {
        item: "github",
        kind: "mcp-server",
        installations: 1,
        requests: 3,
        tokens: { count: 300, measured: false },
        cost_usd: 0,
        unpriced_requests: 3,
        detail: "",
      },
    ],
    total: {
      requests: 7,
      tokens: { count: 1500, measured: false },
      cost_usd: 0.5,
      unpriced_requests: 3,
    },
    notes: ["Estimated: from listing sizes."],
    warnings: ["`skill:gone` matched nothing installed"],
  };

  it("shows no chips and no without table while the list is empty", async () => {
    const { el } = await render();
    expect(el.querySelector(".chips")).toBeNull();
    expect(el.textContent).not.toContain("What if removed");
    expect(lastArgs["without"]).toEqual([]);
  });

  it("shows one removable chip per item and asks the analysis with the list", async () => {
    const { fixture, el } = await render((w) => {
      w.add("skill", "demo");
      w.add("mcp-server", "github");
    });
    const chips = [...el.querySelectorAll(".chip")].map((c) =>
      c.textContent!.replace(/\s+/g, " ").trim(),
    );
    expect(chips).toEqual(["skill:demo", "mcp-server:github"]);
    expect(lastArgs["without"]).toEqual(["skill:demo", "mcp-server:github"]);
    el.querySelector<HTMLButtonElement>('.chip button[aria-label="Remove skill:demo"]')!.click();
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(TestBed.inject(WithoutService).items()).toEqual(["mcp-server:github"]);
    expect(lastArgs["without"]).toEqual(["mcp-server:github"]);
  });

  type View = Awaited<ReturnType<typeof render>>;

  const settle = async ({ fixture }: View) => {
    // An error of the analysis resets the what-if, which starts a second request.
    for (let i = 0; i < 3; i++) {
      fixture.detectChanges();
      await fixture.whenStable();
    }
    fixture.detectChanges();
  };

  const input = (el: HTMLElement) => el.querySelector<HTMLInputElement>("form input[hx-input]")!;
  const button = (el: HTMLElement, name: string) =>
    [...el.querySelectorAll<HTMLButtonElement>("form button[hx-button]")].find(
      (b) => b.textContent!.trim() === name,
    )!;

  async function type(view: View, text: string) {
    input(view.el).value = text;
    input(view.el).dispatchEvent(new Event("input", { bubbles: true }));
    await settle(view);
  }

  /** The text of the elements `aria-describedby` of the input names. */
  const description = (el: HTMLElement) =>
    (input(el).getAttribute("aria-describedby") ?? "")
      .split(" ")
      .map((id) => el.querySelector(`[id="${id}"]`)?.textContent?.trim() ?? "")
      .join(" ");

  it("labels both fields, describes the input and has one filled primary button", async () => {
    const view = await render();
    await settle(view);
    const { el } = view;
    const labels = [...el.querySelectorAll("form hx-field label")];
    expect(labels.map((l) => l.textContent!.trim())).toEqual([
      "Group by",
      "What if everything ran on…",
    ]);
    expect(labels[1].getAttribute("for")).toBe(input(el).id);
    expect(description(el)).toContain("model id prefix");
    expect(input(el).getAttribute("aria-invalid")).toBeNull();
    const primary = [...el.querySelectorAll("form button[hx-button]")].filter(
      (b) => !b.classList.contains("hx-button-text"),
    );
    expect(primary.map((b) => b.textContent!.trim())).toEqual(["Compare"]);
    expect(button(el, "Compare").type).toBe("submit");
    expect(button(el, "Clear").type).toBe("button");
    expect(button(el, "Clear").disabled).toBe(true);
  });

  it("regroups by the choice of the select", async () => {
    const view = await render();
    view.el.querySelector<HTMLElement>('hx-select [role="combobox"]')!.click();
    await settle(view);
    [...document.querySelectorAll<HTMLElement>('[role="option"]')]
      .find((o) => o.textContent!.trim() === "Branch")!
      .click();
    await settle(view);
    expect(lastArgs["by"]).toBe("branch");
  });

  it("applies the what-if by Enter, through the submit button of the form", async () => {
    const view = await render();
    await type(view, "  model-x ");
    // jsdom has no implicit submission: Enter submits through the form's submit button.
    const form = input(view.el).form!;
    form.requestSubmit(form.querySelector<HTMLButtonElement>('button[type="submit"]'));
    await settle(view);
    expect(lastArgs["whatIf"]).toBe("model-x");
  });

  it("applies the what-if by Compare and resets it by Clear", async () => {
    const view = await render();
    await type(view, "model-x");
    button(view.el, "Compare").click();
    await settle(view);
    expect(lastArgs["whatIf"]).toBe("model-x");
    button(view.el, "Clear").click();
    await settle(view);
    expect(lastArgs["whatIf"]).toBeNull();
    expect(input(view.el).value).toBe("");
    expect(button(view.el, "Clear").disabled).toBe(true);
  });

  it("shows the error of an empty entry below the field until the entry changes", async () => {
    const view = await render();
    await type(view, "   ");
    button(view.el, "Compare").click();
    await settle(view);
    expect(input(view.el).getAttribute("aria-invalid")).toBe("true");
    expect(description(view.el)).toContain("Enter a model id prefix.");
    expect(lastArgs["whatIf"]).toBeNull();
    await type(view, "m");
    expect(input(view.el).getAttribute("aria-invalid")).toBeNull();
    expect(view.el.textContent).not.toContain("Enter a model id prefix.");
  });

  it("shows a model without a price below the field and keeps the cost table", async () => {
    const view = await render();
    await type(view, "nope");
    button(view.el, "Compare").click();
    await settle(view);
    expect(input(view.el).getAttribute("aria-invalid")).toBe("true");
    expect(description(view.el)).toContain("no price for the what-if model `nope`");
    expect(input(view.el).value).toBe("nope");
    expect(lastArgs["whatIf"]).toBeNull();
    expect(firstColumn(view.el)).toHaveLength(3);
    expect(button(view.el, "Clear").disabled).toBe(false);
    button(view.el, "Clear").click();
    await settle(view);
    expect(input(view.el).getAttribute("aria-invalid")).toBeNull();
    expect(input(view.el).value).toBe("");
  });

  it("leaves a failure that is not about the entry to the status of the view", async () => {
    failure = "cannot read the price table";
    const view = await render();
    await settle(view);
    expect(input(view.el).getAttribute("aria-invalid")).toBeNull();
    expect(view.el.textContent).not.toContain("cannot read the price table");
    expect(firstColumn(view.el)).not.toContain("model-a");
  });

  it("shows the without table with tildes, a total row, the notes and the warnings", async () => {
    withoutReport = report;
    const { el } = await render((w) => w.add("skill", "demo"));
    const table = el.querySelector(
      'hx-data-table[ariaLabel="What if removed"], hx-data-table:last-of-type',
    )!;
    const rows = [...table.querySelectorAll("tbody tr")].map((r) =>
      [...r.querySelectorAll("td")].map((c) => c.textContent!.trim()),
    );
    expect(rows).toEqual([
      ["demo", "skill", "2", "4", "~1.20 k", "~$0.50"],
      ["github", "mcp-server", "1", "3", "~300", "n/a"],
      ["Total", "", "", "7", "~1.50 k", "~$0.50"],
    ]);
    expect(el.textContent).toContain("Estimated: from listing sizes.");
    expect(el.textContent).toContain("`skill:gone` matched nothing installed");
  });
});

describe("Cost states and estimates", () => {
  beforeEach(() => {
    localStorage.clear();
    rows = baseRows;
    withoutReport = null;
    whatIfReport = null;
    pending = false;
    failure = "";
    TestBed.configureTestingModule({
      providers: [{ provide: Ipc, useClass: FakeIpc }, provideRouter([])],
    });
    TestBed.overrideComponent(Cost, {
      set: {
        imports: [HxDataTable, HxCell, HxField, HxSelect, HxInput, HxButton, HxChip],
        schemas: [NO_ERRORS_SCHEMA],
      },
    });
  });
  afterEach(() => TestBed.resetTestingModule());

  async function render(wait = true) {
    const fixture = TestBed.createComponent(Cost);
    fixture.detectChanges();
    if (wait) await fixture.whenStable();
    fixture.detectChanges();
    return fixture.nativeElement as HTMLElement;
  }

  const table = (el: HTMLElement) => el.querySelector("hx-data-table")!;

  it("shows the table as busy while loading, not as empty", async () => {
    pending = true;
    const el = await render(false);
    expect(table(el).querySelector('[aria-busy="true"]')).not.toBeNull();
    expect(table(el).textContent).not.toContain("No usage found");
  });

  it("names the next action when there is no usage and shows no total", async () => {
    rows = [];
    const el = await render();
    expect(table(el).textContent).toContain(
      "No usage found. Choose a longer period or other filters.",
    );
    expect(el.querySelector(".total")).toBeNull();
  });

  it("does not claim there is no usage after a failure", async () => {
    failure = "cannot read the price table";
    const el = await render();
    expect(table(el).textContent).toContain("Cost could not be loaded. Use Refresh to try again.");
    expect(table(el).textContent).not.toContain("No usage found");
  });

  it("marks an estimated total", async () => {
    rows = [row({ estimated_usd: 2 })];
    const el = await render();
    expect(el.querySelector(".total .est")!.textContent).toBe("~$2.00");
  });

  it("leaves a measured total unmarked", async () => {
    rows = [row({ measured_usd: 2 })];
    const el = await render();
    expect(el.querySelector(".total")!.textContent).toContain("$2.00");
    expect(el.querySelector(".total .est")).toBeNull();
  });

  it("writes the what-if result with tildes, n/a for excluded rows and a Total row", async () => {
    whatIfReport = {
      target: "model-x",
      rows: [
        { model: "model-a", requests: 2, actual_usd: 3, what_if_usd: 1, excluded_requests: 0 },
        { model: "model-c", requests: 0, actual_usd: 0, what_if_usd: 0, excluded_requests: 3 },
      ],
      total: { model: "total", requests: 2, actual_usd: 3, what_if_usd: 1, excluded_requests: 3 },
    };
    const el = await render();
    const result = el.querySelectorAll("hx-data-table")[1];
    const cells = [...result.querySelectorAll("tbody tr")].map((r) =>
      [...r.querySelectorAll("td")].map((c) => c.textContent!.trim()),
    );
    expect(cells).toEqual([
      ["model-a", "2", "~$3.00", "~$1.00", "-~$2.00", "0"],
      ["model-c", "0", "n/a", "n/a", "n/a", "3"],
      ["Total", "2", "~$3.00", "~$1.00", "-~$2.00", "3"],
    ]);
    const marked = [...result.querySelectorAll("tbody .est")].map((s) => s.textContent!.trim());
    expect(marked).toHaveLength(6);
    expect(marked).not.toContain("n/a");
  });
});

describe("withoutViews", () => {
  it("puts the total last and writes n/a for a row without any price", () => {
    const views = withoutViews({
      rows: [],
      total: {
        requests: 2,
        tokens: { count: 5, measured: false },
        cost_usd: 0,
        unpriced_requests: 2,
      },
      notes: [],
      warnings: [],
    });
    expect(views).toHaveLength(1);
    expect(views[0]).toMatchObject({ item: "Total", costText: "n/a", tokens: 5 });
  });
});

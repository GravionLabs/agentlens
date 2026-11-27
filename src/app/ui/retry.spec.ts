// @vitest-environment jsdom
import { ErrorHandler, NO_ERRORS_SCHEMA, type Type } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { provideRouter } from "@angular/router";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { FiltersService } from "../core/filters.service";
import { Ipc } from "../core/ipc";
import sessions from "../core/samples/sessions.json";
import { Advice } from "../features/advice/advice";
import { Compare } from "../features/compare/compare";
import { Context } from "../features/context/context";
import { Cost } from "../features/cost/cost";
import { Efficiency } from "../features/efficiency/efficiency";
import { Flow } from "../features/flow/flow";
import { Overview } from "../features/overview/overview";
import { Skills } from "../features/skills/skills";
import { Timeline } from "../features/timeline/timeline";
import { Unused } from "../features/unused/unused";

/** The commands that fail; every other one answers with the session list. */
let failing: (command: string) => boolean;
let calls: string[];
let thrown: unknown[];

class FailingIpc {
  async invoke<T>(command: string): Promise<T> {
    calls.push(command);
    if (failing(command)) throw new Error("boom");
    return structuredClone(sessions) as T;
  }
}

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
beforeEach(() => {
  localStorage.clear();
  failing = () => true;
  calls = [];
  thrown = [];
  TestBed.configureTestingModule({
    providers: [
      { provide: Ipc, useClass: FailingIpc },
      { provide: ErrorHandler, useValue: { handleError: (e: unknown) => thrown.push(e) } },
      provideRouter([]),
    ],
  });
});
afterEach(() => TestBed.resetTestingModule());

/**
 * Renders a view without its children, so that `(retry)` listens for a DOM event of that name:
 * the JIT compiler of the specs does not know signal outputs.
 */
async function render(view: Type<unknown>) {
  TestBed.overrideComponent(view, { set: { imports: [], schemas: [NO_ERRORS_SCHEMA] } });
  TestBed.inject(FiltersService).compare.set(true);
  const fixture = TestBed.createComponent(view);
  const settle = async () => {
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
  };
  await settle();
  const retry = async (index: number) => {
    const all = (fixture.nativeElement as HTMLElement).querySelectorAll(
      "app-status, app-trend-cards",
    );
    calls = [];
    all[index].dispatchEvent(new Event("retry"));
    await settle();
    return calls;
  };
  return { retry, settle };
}

describe("retry of a failed analysis", () => {
  // The commands behind the status elements of each view, in the order of its template.
  it.each<[string, Type<unknown>, string[]]>([
    ["advice", Advice, ["advice", "tips"]],
    ["compare", Compare, ["compare"]],
    ["context", Context, ["context"]],
    ["cost", Cost, ["cost", "trend"]],
    ["efficiency", Efficiency, ["efficiency", "trend"]],
    ["flow", Flow, ["flow"]],
    ["overview", Overview, ["overview", "trend"]],
    ["skills", Skills, ["skills", "trend"]],
    ["unused", Unused, ["unused"]],
    ["timeline (session list)", Timeline, ["sessions"]],
  ])("runs the analysis of %s again", async (_name, view, commands) => {
    const { retry } = await render(view);
    for (const [index, command] of commands.entries()) {
      expect(await retry(index)).toEqual([command]);
    }
    expect(thrown).toEqual([]);
  });

  it("runs the analysis of the chosen session of the timeline again", async () => {
    failing = (command) => command === "timeline";
    const { retry, settle } = await render(Timeline);
    await settle();
    expect(await retry(1)).toEqual(["timeline"]);
    expect(thrown).toEqual([]);
  });
});

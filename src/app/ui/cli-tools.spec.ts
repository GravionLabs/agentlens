// @vitest-environment jsdom
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { TestBed } from "@angular/core/testing";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { Ipc } from "../core/ipc";
import sample from "../core/samples/cli_tools.json";
import type { Answer, CliTools as CliToolsData } from "../core/types";
import { CliTools } from "./cli-tools";

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

const base = sample.data as CliToolsData;

function answer(data: Partial<CliToolsData>, errors: string[] = []): Answer<CliToolsData> {
  return { schema_version: 2, data: { ...base, ...data }, warnings: 0, errors };
}

const offered = answer({ offered: true });

async function render(invoke: (command: string) => Promise<unknown>) {
  TestBed.configureTestingModule({ providers: [{ provide: Ipc, useValue: { invoke } }] });
  const fixture = TestBed.createComponent(CliTools);
  const el = fixture.nativeElement as HTMLElement;
  const settle = async () => {
    await fixture.whenStable();
    fixture.detectChanges();
  };
  await settle();
  const text = () => el.textContent ?? "";
  const button = (label: string) =>
    Array.from(el.querySelectorAll("button")).find((b) => (b.textContent ?? "").trim() === label);
  const click = async (label: string) => {
    button(label)?.click();
    await settle();
  };
  return { el, text, button, click };
}

describe("CliTools", () => {
  it("shows nothing when the backend does not offer the tools (sample data)", async () => {
    const { el } = await render(async () => sample);
    expect(el.querySelector("button")).toBeNull();
  });

  it("shows nothing when the status cannot be read", async () => {
    const { el } = await render(async () => {
      throw "no status";
    });
    expect(el.querySelector("button")).toBeNull();
  });

  it("offers install and remove when the backend offers the tools", async () => {
    const { button, click } = await render(async () => offered);
    expect(button("Command line tools")).toBeTruthy();
    await click("Command line tools");
    expect(button("Install command line tools")).toBeTruthy();
    expect(button("Remove command line tools")).toBeTruthy();
  });

  it("shows the links, the foreign file, the PATH lines and the note after installing", async () => {
    const installed = answer({
      offered: true,
      created_dir: true,
      tools: [
        { ...base.tools[0], state: "installed", action: "created" },
        { ...base.tools[1], state: "foreign", action: "skipped_foreign" },
      ],
      foreign: ["/home/user/.local/bin/agentlens-mcp"],
      note: "The tools are copies; delete a stale old copy by hand.",
    });
    const invoke = vi.fn(async (command: string) =>
      command === "cli_tools_install" ? installed : offered,
    );
    const { text, click } = await render(invoke);
    await click("Command line tools");
    await click("Install command line tools");
    expect(invoke).toHaveBeenCalledWith("cli_tools_install");
    expect(text()).toContain("linked");
    expect(text()).toContain("Created the folder /home/user/.local/bin");
    expect(text()).toContain("Not touched: /home/user/.local/bin/agentlens-mcp");
    expect(text()).toContain("is not on your PATH");
    expect(text()).toContain("~/.zshrc");
    expect(text()).toContain("~/.bashrc");
    expect(text()).toContain('export PATH="$HOME/.local/bin:$PATH"');
    expect(text()).toContain("delete a stale old copy by hand");
  });

  it("says copied for the AppImage method and no profile line when on PATH", async () => {
    const copied = answer({
      offered: true,
      method: "copy",
      on_path: true,
      profile_lines: [],
      tools: base.tools.map((t) => ({
        ...t,
        state: "installed" as const,
        action: "created" as const,
      })),
    });
    const { text, click } = await render(async (command) =>
      command === "cli_tools_install" ? copied : offered,
    );
    await click("Command line tools");
    await click("Install command line tools");
    expect(text()).toContain("copied");
    expect(text()).toContain("/home/user/.local/bin is on your PATH");
    expect(text()).not.toContain("export PATH");
  });

  it("shows the removal result", async () => {
    const removed = answer({
      offered: true,
      tools: base.tools.map((t) => ({ ...t, action: "removed" as const })),
    });
    const { text, click } = await render(async (command) =>
      command === "cli_tools_remove" ? removed : offered,
    );
    await click("Command line tools");
    await click("Remove command line tools");
    expect(text()).toContain("removed");
  });

  it("shows a tool's error once and other errors of the answer", async () => {
    const failed = answer(
      {
        offered: true,
        tools: [{ ...base.tools[0], action: "failed", error: "permission denied" }, base.tools[1]],
      },
      ["permission denied", "the folder could not be created"],
    );
    const { el, text, click } = await render(async (command) =>
      command === "cli_tools_install" ? failed : offered,
    );
    await click("Command line tools");
    await click("Install command line tools");
    expect(text()).toContain("failed");
    expect(text().split("permission denied")).toHaveLength(2);
    const alerts = Array.from(el.querySelectorAll('[role="alert"]')).map((a) => a.textContent);
    expect(alerts.map((a) => a?.trim())).toEqual(["the folder could not be created"]);
  });

  it("shows a failed command as an error and keeps the buttons usable", async () => {
    const { el, button, click } = await render(async (command) => {
      if (command === "cli_tools_install") throw "Could not install: read-only file system";
      return offered;
    });
    await click("Command line tools");
    await click("Install command line tools");
    expect(el.querySelector('[role="alert"]')?.textContent).toContain("read-only file system");
    expect(button("Install command line tools")?.disabled).toBe(false);
  });
});

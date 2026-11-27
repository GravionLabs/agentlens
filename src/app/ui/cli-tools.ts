import { ChangeDetectionStrategy, Component, computed, inject, signal } from "@angular/core";
import { HxButton, HxMessage } from "@gravionlabs/helix-ui";
import { messageOf } from "../core/analysis";
import { Ipc } from "../core/ipc";
import type { Answer, CliTool, CliTools as CliToolsData } from "../core/types";

type Busy = "install" | "remove" | null;

/**
 * “Command line tools” in the status bar: installs or removes the links to the bundled CLI and MCP
 * server in a folder on PATH, or on Windows the folder of AgentLens as an entry of the user PATH. Only
 * shown where the backend offers it (macOS, AppImage and Windows).
 */
@Component({
  selector: "app-cli-tools",
  imports: [HxButton, HxMessage],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./cli-tools.html",
  styles: `
    :host {
      position: relative;
      display: inline-flex;
      align-items: center;
    }
    .panel {
      position: absolute;
      bottom: 100%;
      right: 0;
      z-index: 5;
      margin-bottom: 6px;
      padding: 10px 12px;
      width: min(480px, 90vw);
      /* The status bar does not wrap its text; the panel does. */
      white-space: normal;
      background: var(--h-surface-overlay);
      border: 1px solid var(--h-border-panel);
      border-radius: 6px;
      color: var(--h-text-color);
      user-select: text;
    }
    .actions {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }
    .tools,
    .lines {
      list-style: none;
      margin: 8px 0;
      padding: 0;
    }
    .tools li,
    .lines li {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      align-items: baseline;
      margin: 2px 0;
    }
    p {
      margin: 6px 0;
    }
    .note {
      font-size: var(--h-font-size-sm);
      color: var(--h-text-muted-color);
    }
    .path,
    .line {
      overflow-wrap: anywhere;
    }
    .err {
      color: var(--h-tag-warn-color);
    }
  `,
})
export class CliTools {
  private readonly ipc = inject(Ipc);

  protected readonly open = signal(false);
  protected readonly busy = signal<Busy>(null);
  /** The latest answer of the status, install or remove command; null until one arrived. */
  protected readonly state = signal<CliToolsData | null>(null);
  protected readonly errors = signal<string[]>([]);
  protected readonly offered = computed(() => this.state()?.offered === true);

  constructor() {
    void this.load();
  }

  private async load(): Promise<void> {
    try {
      const answer = await this.ipc.invoke<Answer<CliToolsData>>("cli_tools_status");
      this.state.set(answer?.data ?? null);
    } catch {
      // No status, no action: the button stays hidden.
      this.state.set(null);
    }
  }

  protected async run(action: "install" | "remove"): Promise<void> {
    if (this.busy()) return;
    this.busy.set(action);
    this.errors.set([]);
    try {
      const answer = await this.ipc.invoke<Answer<CliToolsData>>(`cli_tools_${action}`);
      if (answer?.data) this.state.set(answer.data);
      // Each tool's error is shown with the tool (the answer has it as "<path>: <error>"); the rest
      // stands on its own.
      const shown = new Set(
        (answer?.data?.tools ?? [])
          .filter((t) => t.error !== null)
          .map((t) => `${t.path}: ${t.error}`),
      );
      this.errors.set((answer?.errors ?? []).filter((e) => !shown.has(e)));
    } catch (error) {
      this.errors.set([messageOf(error)]);
    } finally {
      this.busy.set(null);
    }
  }

  protected userPath(s: CliToolsData): boolean {
    return s.method === "user_path";
  }

  /** Whether the call changed the user PATH (Windows), so that running programs have an old one. */
  protected changed(s: CliToolsData): boolean {
    return s.tools.some((t) => t.action === "created" || t.action === "removed");
  }

  protected describe(tool: CliTool, s: CliToolsData): string {
    if (s.method === "user_path") return describeUserPath(tool);
    const copy = s.method === "copy";
    switch (tool.action) {
      case "created":
        return copy ? "copied" : "linked";
      case "repaired":
        return copy ? "copy replaced" : "link repaired";
      case "kept":
        return "already installed";
      case "removed":
        return "removed";
      case "absent":
        return "was not installed";
      case "skipped_foreign":
        return "not touched (not created by AgentLens)";
      case "failed":
        return "failed";
      case null:
        if (tool.state === "stale") {
          return copy ? "an old copy, install replaces it" : "an old link, install repairs it";
        }
        return STATES[tool.state];
    }
  }
}

/** The labels on Windows, where the tool is installed while its folder is on the user PATH. */
function describeUserPath(tool: CliTool): string {
  switch (tool.action) {
    case "created":
      return "added to PATH";
    case "kept":
      return "already on PATH";
    case "removed":
      return "removed from PATH";
    case "absent":
      return "was not on PATH";
    case "failed":
      return "failed";
    default:
      return tool.state === "installed" ? "on PATH" : "not on PATH";
  }
}

const STATES: Record<Exclude<CliTool["state"], "stale">, string> = {
  missing: "not installed",
  installed: "installed",
  foreign: "a file not created by AgentLens is in the way",
};

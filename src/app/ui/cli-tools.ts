import { ChangeDetectionStrategy, Component, computed, inject, signal } from "@angular/core";
import { HxMessage } from "@gravionlabs/helix-ui";
import { messageOf } from "../core/analysis";
import { Ipc } from "../core/ipc";
import type { Answer, CliTool, CliTools as CliToolsData } from "../core/types";

type Busy = "install" | "remove" | null;

/**
 * “Command line tools” in the status bar: installs or removes the links to the bundled CLI and MCP
 * server in a folder on PATH. Only shown where the backend offers it (macOS and AppImage).
 */
@Component({
  selector: "app-cli-tools",
  imports: [HxMessage],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (offered()) {
      <button
        type="button"
        class="btn btn-link btn-sm trigger"
        [attr.aria-expanded]="open()"
        (click)="open.set(!open())"
      >
        Command line tools
      </button>
      @if (open()) {
        <div class="panel" role="dialog" aria-label="Command line tools">
          <div class="actions">
            <button
              type="button"
              class="btn btn-sm"
              [disabled]="busy() !== null"
              (click)="run('install')"
            >
              Install command line tools
            </button>
            <button
              type="button"
              class="btn btn-sm"
              [disabled]="busy() !== null"
              (click)="run('remove')"
            >
              Remove command line tools
            </button>
          </div>
          @if (busy() === "install") {
            <hx-message severity="info" variant="simple" size="small"
              >Installing the command line tools…</hx-message
            >
          } @else if (busy() === "remove") {
            <hx-message severity="info" variant="simple" size="small"
              >Removing the command line tools…</hx-message
            >
          }
          @if (state(); as s) {
            <ul class="tools">
              @for (t of s.tools; track t.name) {
                <li>
                  <code>{{ t.name }}</code>
                  <span class="note">{{ describe(t, s) }}</span>
                  <span class="note path">{{ t.path }}</span>
                  @if (t.error) {
                    <span class="note err">{{ t.error }}</span>
                  }
                </li>
              }
            </ul>
            @if (s.created_dir) {
              <hx-message severity="info" variant="simple" size="small"
                >Created the folder {{ s.dir }}.</hx-message
              >
            }
            @for (f of s.foreign; track f) {
              <hx-message severity="warn" variant="simple" size="small">
                Not touched: {{ f }} is a file AgentLens did not create. Remove or rename it by hand
                to install the tool there.
              </hx-message>
            }
            @if (s.on_path) {
              <hx-message severity="info" variant="simple" size="small"
                >{{ s.dir }} is on your PATH.</hx-message
              >
            } @else {
              <hx-message severity="warn" variant="simple" size="small">
                {{ s.dir }} is not on the PATH AgentLens was started with. If your shell does not
                have it either, add this line to your shell profile:
              </hx-message>
              <ul class="lines">
                @for (p of s.profile_lines; track p.shell) {
                  <li>
                    <span class="note">{{ p.shell }} ({{ p.file }})</span>
                    <code class="line">{{ p.line }}</code>
                  </li>
                }
              </ul>
            }
            @if (s.note) {
              <hx-message severity="info" variant="simple" size="small">{{ s.note }}</hx-message>
            }
          }
          @for (e of errors(); track $index) {
            <hx-message severity="error" variant="simple" size="small">{{ e }}</hx-message>
          }
        </div>
      }
    }
  `,
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
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 6px;
      color: var(--text);
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
      font-size: 0.85rem;
      color: var(--muted);
    }
    .path,
    .line {
      overflow-wrap: anywhere;
    }
    .err {
      color: var(--warn);
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

  protected describe(tool: CliTool, s: CliToolsData): string {
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

const STATES: Record<Exclude<CliTool["state"], "stale">, string> = {
  missing: "not installed",
  installed: "installed",
  foreign: "a file not created by AgentLens is in the way",
};

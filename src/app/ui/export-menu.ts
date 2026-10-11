import { ChangeDetectionStrategy, Component, inject, input, signal } from "@angular/core";
import { SAMPLE_MODE } from "../core/ipc";
import { ExportFormat, ExportService } from "../core/export.service";

const LABELS: Record<ExportFormat, string> = {
  json: "JSON",
  html: "HTML report",
  csv: "CSV",
};

/** “Export” next to a view's title: the formats the view supports, then a save dialog. */
@Component({
  selector: "app-export-menu",
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <button
      type="button"
      class="trigger"
      [disabled]="sampleMode || exporter.status().kind === 'saving'"
      [attr.aria-expanded]="open()"
      [title]="sampleMode ? 'Exporting needs the application; this page shows sample data.' : ''"
      (click)="open.set(!open())"
    >
      Export ▾
    </button>
    @if (open() && !sampleMode) {
      <ul class="menu" role="menu">
        @for (f of formats(); track f) {
          <li role="none">
            <button type="button" role="menuitem" (click)="choose(f)">{{ label(f) }}</button>
          </li>
        }
      </ul>
    }
    @switch (exporter.status().kind) {
      @case ("saving") {
        <span class="note">Saving…</span>
      }
      @case ("done") {
        <span class="note" role="status">Saved {{ savedPath() }}</span>
      }
      @case ("error") {
        <span class="note err" role="alert">{{ errorMessage() }}</span>
      }
    }
  `,
  styles: `
    :host {
      position: relative;
      display: inline-flex;
      align-items: baseline;
      gap: 8px;
    }
    .trigger {
      all: unset;
      cursor: pointer;
      color: var(--accent);
      font-size: 0.85rem;
    }
    .trigger[disabled] {
      color: var(--muted);
      cursor: default;
    }
    .menu {
      position: absolute;
      top: 100%;
      left: 0;
      z-index: 5;
      margin: 4px 0 0;
      padding: 4px;
      list-style: none;
      min-width: 140px;
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 6px;
    }
    .menu button {
      all: unset;
      box-sizing: border-box;
      display: block;
      width: 100%;
      padding: 4px 8px;
      border-radius: 4px;
      cursor: pointer;
    }
    .menu button:hover {
      background: color-mix(in srgb, var(--accent) 12%, transparent);
    }
    .note {
      font-size: 0.85rem;
      color: var(--muted);
      max-width: 340px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .err {
      color: var(--warn);
    }
  `,
})
export class ExportMenu {
  readonly view = input.required<string>();
  readonly formats = input<ExportFormat[]>(["json"]);
  /** Options of the view that change what is analyzed (the grouping of the cost view, ...). */
  readonly options = input<Record<string, unknown> | undefined>(undefined);

  protected readonly exporter = inject(ExportService);
  protected readonly sampleMode = inject(SAMPLE_MODE);
  protected readonly open = signal(false);

  protected label(format: ExportFormat): string {
    return LABELS[format];
  }

  protected savedPath(): string {
    const s = this.exporter.status();
    return s.kind === "done" ? s.path : "";
  }

  protected errorMessage(): string {
    const s = this.exporter.status();
    return s.kind === "error" ? s.message : "";
  }

  protected choose(format: ExportFormat): void {
    this.open.set(false);
    void this.exporter.export(this.view(), format, this.options());
  }
}

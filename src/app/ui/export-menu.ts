import { ChangeDetectionStrategy, Component, computed, inject, input } from "@angular/core";
import { HxButton, HxMenu, type HxMenuItem } from "@gravionlabs/helix-ui";
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
  imports: [HxButton, HxMenu],
  template: `
    <button
      hx-button
      type="button"
      variant="link"
      size="small"
      aria-haspopup="menu"
      [attr.aria-expanded]="menu.open()"
      [disabled]="sampleMode || exporter.status().kind === 'saving'"
      [title]="sampleMode ? 'Exporting needs the application; this page shows sample data.' : ''"
      (click)="menu.toggle($event)"
    >
      Export ▾
    </button>
    <hx-menu #menu popup ariaLabel="Export" [model]="items()" />
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
      display: inline-flex;
      align-items: baseline;
      gap: 8px;
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
  protected readonly items = computed<HxMenuItem[]>(() =>
    this.formats().map((f) => ({ label: LABELS[f], command: () => this.choose(f) })),
  );

  protected savedPath(): string {
    const s = this.exporter.status();
    return s.kind === "done" ? s.path : "";
  }

  protected errorMessage(): string {
    const s = this.exporter.status();
    return s.kind === "error" ? s.message : "";
  }

  protected choose(format: ExportFormat): void {
    void this.exporter.export(this.view(), format, this.options());
  }
}

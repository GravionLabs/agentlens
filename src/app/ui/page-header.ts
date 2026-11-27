import { ChangeDetectionStrategy, Component, inject, input, signal } from "@angular/core";
import { RouterLink } from "@angular/router";
import { HelixPageHeader } from "@gravionlabs/helix-shell";
import { HxButton } from "@gravionlabs/helix-ui";
import { FiltersService } from "../core/filters.service";
import type { ExportFormat } from "../core/export.service";
import { ExportMenu } from "./export-menu";

/** Title and a one-line summary; the longer explanation is folded away behind “Explain”. */
@Component({
  selector: "app-page-header",
  imports: [RouterLink, ExportMenu, HelixPageHeader, HxButton],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./page-header.html",
  styles: `
    helix-page-header {
      margin-bottom: 4px;
    }
    .actions {
      display: flex;
      align-items: center;
      gap: 14px;
    }
    .window {
      color: var(--h-text-muted-color);
      font-size: var(--h-font-size-sm);
    }
    .details {
      margin: 8px 0;
      padding: 8px 14px;
      background: var(--h-surface-card);
      border: 1px solid var(--h-border-panel);
      border-left: 3px solid var(--h-primary-color);
      border-radius: 6px;
      color: var(--h-text-muted-color);
      font-size: var(--h-font-size-sm);
    }
    .details ::ng-deep p {
      margin: 4px 0;
    }
    .more {
      display: inline-block;
      margin-top: 4px;
      font-size: var(--h-font-size-sm);
      color: var(--h-primary-color);
    }
  `,
})
export class PageHeader {
  readonly title = input.required<string>();
  readonly summary = input.required<string>();
  /** The view to offer an export for (the name of its backend analysis); none shows no menu. */
  readonly exportView = input<string>();
  readonly exportFormats = input<ExportFormat[]>(["json"]);
  readonly exportOptions = input<Record<string, unknown>>();
  protected readonly filters = inject(FiltersService);
  protected readonly open = signal(false);
}

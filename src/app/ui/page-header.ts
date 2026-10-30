import { ChangeDetectionStrategy, Component, inject, input, signal } from "@angular/core";
import { RouterLink } from "@angular/router";
import { HelixPageHeader } from "@gravionlabs/helix-shell";
import { FiltersService } from "../core/filters.service";
import type { ExportFormat } from "../core/export.service";
import { ExportMenu } from "./export-menu";

/** Title and a one-line summary; the longer explanation is folded away behind “Explain”. */
@Component({
  selector: "app-page-header",
  imports: [RouterLink, ExportMenu, HelixPageHeader],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <helix-page-header [title]="title()" [subtitle]="summary()" [headingLevel]="2">
      <div helixPageActions class="actions">
        <span class="window" data-testid="window">{{ filters.windowLabel() }}</span>
        <button
          type="button"
          class="explain"
          [attr.aria-expanded]="open()"
          [attr.aria-controls]="'details-' + title()"
          (click)="open.set(!open())"
        >
          {{ open() ? "Hide" : "Explain" }}
        </button>
        @if (exportView(); as view) {
          <app-export-menu [view]="view" [formats]="exportFormats()" [options]="exportOptions()" />
        }
      </div>
    </helix-page-header>
    @if (open()) {
      <div class="details" [id]="'details-' + title()">
        <ng-content />
        <a class="more" routerLink="/glossary">Open the glossary</a>
      </div>
    }
  `,
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
      color: var(--muted);
      font-size: 0.85rem;
    }
    .explain {
      all: unset;
      cursor: pointer;
      color: var(--accent);
      font-size: 0.85rem;
      text-decoration: underline;
    }
    .explain:focus-visible {
      outline: 2px solid var(--accent);
    }
    .details {
      margin: 8px 0;
      padding: 8px 14px;
      background: var(--panel);
      border: 1px solid var(--line);
      border-left: 3px solid var(--accent);
      border-radius: 6px;
      color: var(--muted);
      font-size: 0.9rem;
    }
    .details ::ng-deep p {
      margin: 4px 0;
    }
    .more {
      display: inline-block;
      margin-top: 4px;
      font-size: 0.85rem;
      color: var(--accent);
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

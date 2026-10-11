import { ChangeDetectionStrategy, Component, inject } from "@angular/core";
import { UpdateService } from "../core/update.service";

/** The update check in the header: a button, and what the check found. */
@Component({
  selector: "app-update-status",
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @switch (updates.state()) {
      @case ("checking") {
        <span class="note">Checking for updates…</span>
      }
      @case ("upToDate") {
        <span class="note">Up to date</span>
      }
      @case ("available") {
        <span class="note" [title]="updates.info()?.notes ?? ''">
          Version {{ updates.info()?.version }} is available
        </span>
        <button type="button" class="btn btn-sm" (click)="updates.install()">
          Install and restart
        </button>
      }
      @case ("installing") {
        <span class="note">Installing the update…</span>
      }
      @case ("error") {
        <span class="note err" [title]="updates.error() ?? ''">Update check failed</span>
      }
    }
    @if (updates.state() !== "available" && updates.state() !== "installing") {
      <button
        type="button"
        class="btn btn-link btn-sm"
        [disabled]="updates.busy()"
        (click)="updates.check()"
      >
        Check for updates
      </button>
    }
  `,
  styles: `
    :host {
      display: inline-flex;
      align-items: center;
      gap: 8px;
    }
    .note {
      font-size: 0.85rem;
      color: var(--muted);
    }
    .err {
      color: var(--warn);
    }
  `,
})
export class UpdateStatus {
  protected readonly updates = inject(UpdateService);
}

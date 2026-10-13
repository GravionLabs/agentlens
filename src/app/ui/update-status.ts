import { ChangeDetectionStrategy, Component, computed, inject } from "@angular/core";
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
        <span class="note err">Update check failed</span>
        <span class="note err detail">{{ updates.error() }}</span>
      }
      @case ("installFailed") {
        <span class="note err">Update failed</span>
        <span class="note err detail">{{ updates.error() }}</span>
        <button type="button" class="btn btn-sm" (click)="updates.install()">
          Install and restart
        </button>
      }
    }
    @if (showCheck()) {
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
    .detail {
      user-select: text;
      cursor: text;
      overflow-wrap: anywhere;
      max-width: 40ch;
      max-height: 4.5em;
      overflow-y: auto;
    }
    .err {
      color: var(--warn);
    }
  `,
})
export class UpdateStatus {
  protected readonly updates = inject(UpdateService);
  /** A new check is not offered while an update is pending: installing is the way forward. */
  protected readonly showCheck = computed(
    () => !["available", "installing", "installFailed"].includes(this.updates.state()),
  );
}

import { ChangeDetectionStrategy, Component, computed, inject } from "@angular/core";
import { HxButton, HxMessage } from "@gravionlabs/helix-ui";
import { UpdateService } from "../core/update.service";

/** The update check in the header: a button, and what the check found. */
@Component({
  selector: "app-update-status",
  imports: [HxButton, HxMessage],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @switch (updates.state()) {
      @case ("checking") {
        <hx-message severity="secondary" variant="simple" size="small"
          >Checking for updates…</hx-message
        >
      }
      @case ("upToDate") {
        <hx-message severity="success" variant="simple" size="small">Up to date</hx-message>
      }
      @case ("available") {
        <hx-message
          severity="info"
          variant="simple"
          size="small"
          [title]="updates.info()?.notes ?? ''"
        >
          Version {{ updates.info()?.version }} is available
        </hx-message>
        <button hx-button type="button" variant="outlined" size="small" (click)="updates.install()">
          Install and restart
        </button>
      }
      @case ("installing") {
        <hx-message severity="info" variant="simple" size="small"
          >Installing the update…</hx-message
        >
      }
      @case ("error") {
        <hx-message severity="error" variant="simple" size="small">
          Update check failed
          <span class="detail">{{ updates.error() }}</span>
        </hx-message>
      }
      @case ("installFailed") {
        <hx-message severity="error" variant="simple" size="small">
          Update failed
          <span class="detail">{{ updates.error() }}</span>
        </hx-message>
        <button hx-button type="button" variant="outlined" size="small" (click)="updates.install()">
          Install and restart
        </button>
      }
    }
    @if (showCheck()) {
      <button
        hx-button
        type="button"
        variant="link"
        size="small"
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
    .detail {
      user-select: text;
      cursor: text;
      overflow-wrap: anywhere;
      max-width: 40ch;
      max-height: 4.5em;
      overflow-y: auto;
      display: inline-block;
      vertical-align: top;
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

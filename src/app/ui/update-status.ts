import { ChangeDetectionStrategy, Component, computed, inject } from "@angular/core";
import { HxButton, HxMessage } from "@gravionlabs/helix-ui";
import { UpdateService } from "../core/update.service";

/** The update check in the status bar: a button, and what the check found. */
@Component({
  selector: "app-update-status",
  imports: [HxButton, HxMessage],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./update-status.html",
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
      max-height: 1.5em;
      white-space: normal;
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

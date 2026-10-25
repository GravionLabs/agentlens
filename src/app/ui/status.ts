import { ChangeDetectionStrategy, Component, computed, input } from "@angular/core";
import { HxMessage, HxProgressSpinner } from "@gravionlabs/helix-ui";
import { messageOf } from "../core/analysis";

/** Loading, failure and the warnings of reading the sessions for one analysis. */
@Component({
  selector: "app-status",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HxMessage, HxProgressSpinner],
  template: `
    @if (loading()) {
      <hx-message severity="secondary" variant="simple" size="small">
        <hx-progress-spinner class="spinner" ariaLabel="Loading" />Reading sessions…
      </hx-message>
    }
    @if (failure(); as message) {
      <hx-message severity="error" variant="simple" size="small">{{ message }}</hx-message>
    }
    @if (warnings() > 0) {
      <hx-message severity="warn" variant="simple" size="small"
        >{{ warnings() }} warnings while reading sessions.</hx-message
      >
    }
    @for (e of errors(); track $index) {
      <hx-message severity="error" variant="simple" size="small">{{ e }}</hx-message>
    }
  `,
  styles: `
    :host {
      display: block;
    }
    hx-message {
      display: block;
      margin: 8px 0;
    }
    .spinner {
      display: inline-block;
      width: 14px;
      height: 14px;
      margin-right: 8px;
      vertical-align: middle;
    }
  `,
})
export class Status {
  readonly loading = input(false);
  readonly error = input<unknown>(undefined);
  readonly warnings = input(0);
  readonly errors = input<string[]>([]);

  protected readonly failure = computed(() => {
    const e = this.error();
    return e === undefined || e === null ? null : messageOf(e);
  });
}

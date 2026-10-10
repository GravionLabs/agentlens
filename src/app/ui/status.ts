import { ChangeDetectionStrategy, Component, computed, input } from "@angular/core";
import { messageOf } from "../core/analysis";

/** Loading, failure and the warnings of reading the sessions for one analysis. */
@Component({
  selector: "app-status",
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (loading()) {
      <p class="status" role="status">
        <span class="spinner" aria-hidden="true"></span>Reading sessions…
      </p>
    }
    @if (failure(); as message) {
      <p class="status error" role="alert">{{ message }}</p>
    }
    @if (warnings() > 0) {
      <p class="status note">{{ warnings() }} warnings while reading sessions.</p>
    }
    @for (e of errors(); track $index) {
      <p class="status error">{{ e }}</p>
    }
  `,
  styles: `
    .status {
      display: flex;
      align-items: center;
      gap: 8px;
      margin: 8px 0;
      color: var(--muted);
    }
    .spinner {
      width: 14px;
      height: 14px;
      border: 2px solid var(--line);
      border-top-color: var(--accent);
      border-radius: 50%;
      animation: spin 0.8s linear infinite;
    }
    @keyframes spin {
      to {
        transform: rotate(360deg);
      }
    }
    @media (prefers-reduced-motion: reduce) {
      .spinner {
        animation-duration: 3s;
      }
    }
    .error {
      color: var(--bad);
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

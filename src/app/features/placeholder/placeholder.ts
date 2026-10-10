import { ChangeDetectionStrategy, Component, input } from "@angular/core";

/** A view that is not built yet. */
@Component({
  selector: "app-placeholder",
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <h2>{{ title() }}</h2>
      <p class="muted">This view is coming soon.</p>
    </section>
  `,
})
export class Placeholder {
  readonly title = input("");
}

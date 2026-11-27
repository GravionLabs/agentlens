import { ChangeDetectionStrategy, Component } from "@angular/core";

/**
 * Lint probe (#559): the inline styles carry a hex color, an off-scale font size, a styled native
 * control and a px font shorthand, so Stylelint reports them through `postcss-angular`.
 * `src/testing/lint-probe.spec.ts` expects every rule; the file is left out of `pnpm lint` and the
 * build.
 */
@Component({
  selector: "app-styles-probe",
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./styles-probe.component.html",
  styles: `
    .probe {
      color: #123456;
      font-size: 13px;
    }
    .btn-probe {
      margin: 0;
    }
    button {
      font: 600 13px sans-serif;
    }
  `,
})
export class StylesProbe {}

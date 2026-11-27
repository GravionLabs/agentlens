import { ChangeDetectionStrategy, Component } from "@angular/core";

/** Lint probe (#542): an inline template outside a spec file fails the guard rule. */
@Component({
  selector: "app-inline-template-probe",
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<p>inline</p>`,
})
export class InlineTemplateProbe {}

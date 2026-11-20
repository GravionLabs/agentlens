import { Component, EventEmitter, OnDestroy, Output } from "@angular/core";
import { Ipc } from "../../app/core/ipc";

/**
 * Lint probe (#542): every violation here is deliberate. `src/testing/lint-probe.spec.ts` runs
 * oxlint and ESLint on this folder and expects each rule id, so a broken oxlint JS plugin, lost
 * type information or a lost template rule fails a test instead of passing silently. Left out of
 * `pnpm lint` and the build.
 */
@Component({
  // @angular-eslint/component-selector: not the app prefix.
  selector: "probe-component",
  templateUrl: "./probe.component.html",
})
export class ProbeComponent implements OnDestroy {
  // @angular-eslint/no-output-on-prefix
  @Output() readonly onPick = new EventEmitter<string>();

  // @angular-eslint/prefer-inject
  constructor(readonly ipc: Ipc) {}

  // @angular-eslint/use-lifecycle-interface: no `implements OnInit`.
  ngOnInit(): void {
    // typescript/no-floating-promises (type-aware)
    this.ipc.invoke("probe");
  }

  // @angular-eslint/no-empty-lifecycle-method
  ngOnDestroy(): void {}

  pick(): void {
    this.onPick.emit("pick");
  }
}

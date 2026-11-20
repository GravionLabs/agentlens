import { Component, Input } from "@angular/core";

/**
 * Stands in for `HelpLink` in specs: signal inputs of app components do not bind without the Angular
 * compiler plugin, so this takes plain inputs and renders the same link the real one does.
 */
@Component({
  selector: "app-help-link",
  templateUrl: "./help-link.testing.html",
})
export class HelpLinkStub {
  @Input() term = "";
  @Input() label = "";
}

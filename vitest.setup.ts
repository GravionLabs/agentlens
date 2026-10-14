// Angular services use JIT compilation under Vitest.
import "@angular/compiler";

// jsdom has no layout: the CDK listbox (helix select) calls scrollIntoView on the active option.
if (typeof Element !== "undefined" && !Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}

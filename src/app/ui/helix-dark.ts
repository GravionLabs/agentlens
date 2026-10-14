import { DOCUMENT } from "@angular/common";
import { DestroyRef, Directive, inject } from "@angular/core";

const QUERY = "(prefers-color-scheme: dark)";

/**
 * Bridges the app's dark mode to helix: toggles the `app-dark` class on `<html>`. Dark when the system
 * prefers dark and `data-theme` is not `light`, or when `data-theme="dark"` (same rule as `styles.scss`).
 */
@Directive({ selector: "[appHelixDark]" })
export class HelixDark {
  constructor() {
    const doc = inject(DOCUMENT);
    const root = doc.documentElement;
    const media = doc.defaultView?.matchMedia(QUERY);
    const apply = () => {
      const theme = root.getAttribute("data-theme");
      const dark = theme === "dark" || (theme !== "light" && !!media?.matches);
      root.classList.toggle("app-dark", dark);
    };
    apply();
    media?.addEventListener("change", apply);
    const observer = new MutationObserver(apply);
    observer.observe(root, { attributes: true, attributeFilter: ["data-theme"] });
    inject(DestroyRef).onDestroy(() => {
      media?.removeEventListener("change", apply);
      observer.disconnect();
    });
  }
}

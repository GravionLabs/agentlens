import { ChangeDetectionStrategy, Component, computed, input } from "@angular/core";
import { RouterLink } from "@angular/router";
import { glossaryEntry } from "../core/glossary";

/** A word with a dotted underline: hover for the short explanation, click for the glossary. */
@Component({
  selector: "app-term",
  imports: [RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<a class="term" routerLink="/glossary" [queryParams]="{ term: id() }" [title]="short()"
    ><ng-content
  /></a>`,
  styles: `
    .term {
      color: inherit;
      text-decoration: underline dotted var(--muted);
      text-underline-offset: 3px;
      cursor: help;
    }
    .term:hover {
      color: var(--accent);
    }
  `,
})
export class Term {
  readonly id = input.required<string>();
  protected readonly short = computed(() => glossaryEntry(this.id())?.short ?? "");
}

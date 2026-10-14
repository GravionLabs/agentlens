import { ChangeDetectionStrategy, Component, input } from "@angular/core";
import { RouterLink } from "@angular/router";
import { glossaryEntry } from "../core/glossary";

/** The tooltip of a header with a glossary term: its short explanation. */
export function termTitle(term: string): string | undefined {
  return glossaryEntry(term)?.short;
}

/** The "?" beside a table header: links to the glossary entry of a term. */
@Component({
  selector: "app-help-link",
  imports: [RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<a
    class="help"
    routerLink="/glossary"
    [queryParams]="{ term: term() }"
    [attr.aria-label]="'Explain ' + label()"
    >?</a
  >`,
  styles: `
    .help {
      margin-left: 4px;
      font-size: 0.7rem;
      color: var(--muted);
      border: 1px solid var(--line);
      border-radius: 50%;
      width: 14px;
      height: 14px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      text-decoration: none;
      vertical-align: middle;
    }
    .help:hover {
      color: var(--accent);
      border-color: var(--accent);
    }
  `,
})
export class HelpLink {
  readonly term = input.required<string>();
  readonly label = input.required<string>();
}

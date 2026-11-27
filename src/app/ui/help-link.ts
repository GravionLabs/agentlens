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
  templateUrl: "./help-link.html",
  styles: `
    .help {
      margin-left: 4px;
      font-size: var(--h-font-size-xs);
      color: var(--h-text-muted-color);
      border: 1px solid var(--h-border-panel);
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
      color: var(--h-primary-color);
      border-color: var(--h-primary-color);
    }
  `,
})
export class HelpLink {
  readonly term = input.required<string>();
  readonly label = input.required<string>();
}

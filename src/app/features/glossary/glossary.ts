import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from "@angular/core";
import { ActivatedRoute } from "@angular/router";
import { HxInput } from "@gravionlabs/helix-ui";
import { toSignal } from "@angular/core/rxjs-interop";
import { GLOSSARY_GROUPS, searchGlossary } from "../../core/glossary";

@Component({
  selector: "app-glossary",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HxInput],
  template: `
    <section class="view">
      <h2>Glossary</h2>
      <p class="muted">What the terms in the tables and charts mean.</p>
      <input
        hx-input
        type="search"
        placeholder="Search terms"
        aria-label="Search terms"
        [value]="query()"
        (input)="query.set(asInput($event).value)"
      />
      @for (g of groups(); track g.name) {
        <h3>{{ g.name }}</h3>
        <dl>
          @for (e of g.entries; track e.id) {
            <div class="entry" [id]="e.id" [class.target]="e.id === focus()">
              <dt>{{ e.term }}</dt>
              <dd>
                <strong>{{ e.short }}</strong>
                <span class="muted">{{ e.long }}</span>
              </dd>
            </div>
          }
        </dl>
      } @empty {
        <p class="muted">No term matches.</p>
      }
    </section>
  `,
  styles: `
    input {
      margin: 8px 0 4px;
      width: min(320px, 100%);
    }
    h3 {
      margin: 18px 0 6px;
      font-size: 1.05rem;
    }
    dl {
      margin: 0;
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
      gap: 8px;
    }
    .entry {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 8px 12px;
      scroll-margin-top: 12px;
    }
    .entry.target {
      border-color: var(--accent);
    }
    dt {
      font-weight: 650;
    }
    dd {
      margin: 2px 0 0;
      display: flex;
      flex-direction: column;
      gap: 2px;
      font-size: 0.9rem;
    }
  `,
})
export class Glossary {
  protected readonly query = signal("");
  private readonly params = toSignal(inject(ActivatedRoute).queryParamMap);
  protected readonly focus = computed(() => this.params()?.get("term") ?? "");

  protected readonly groups = computed(() => {
    const found = searchGlossary(this.query());
    return GLOSSARY_GROUPS.map((name) => ({
      name,
      entries: found.filter((e) => e.group === name),
    })).filter((g) => g.entries.length > 0);
  });

  constructor() {
    // Scroll to the term a link points at once it is rendered.
    effect(() => {
      const id = this.focus();
      if (id) setTimeout(() => document.getElementById(id)?.scrollIntoView({ block: "start" }));
    });
  }

  protected asInput(e: Event): HTMLInputElement {
    return e.target as HTMLInputElement;
  }
}

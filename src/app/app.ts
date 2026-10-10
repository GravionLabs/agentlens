import { ChangeDetectionStrategy, Component } from "@angular/core";
import { RouterLink, RouterLinkActive, RouterOutlet } from "@angular/router";
import { NAV } from "./app.routes";
import { SAMPLE_MODE } from "./core/ipc";
import { FilterBar } from "./ui/filter-bar";
import { inject } from "@angular/core";

@Component({
  selector: "app-root",
  imports: [RouterOutlet, RouterLink, RouterLinkActive, FilterBar],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <header class="top">
      <strong class="brand">AgentLens</strong>
      <nav aria-label="Views">
        @for (item of nav; track item.path) {
          <a [routerLink]="item.path" routerLinkActive="active">{{ item.label }}</a>
        }
      </nav>
      @if (sampleMode) {
        <span class="sample">sample data</span>
      }
    </header>
    <app-filter-bar />
    <main>
      <router-outlet />
    </main>
  `,
  styles: `
    .top {
      display: flex;
      align-items: center;
      gap: 24px;
      padding: 10px 16px;
      background: var(--panel);
      border-bottom: 1px solid var(--line);
    }
    .brand {
      font-size: 1.1rem;
    }
    nav {
      display: flex;
      flex-wrap: wrap;
      gap: 4px;
      flex: 1;
    }
    nav a {
      color: var(--muted);
      text-decoration: none;
      padding: 4px 10px;
      border-radius: 6px;
    }
    nav a:hover {
      color: var(--text);
    }
    nav a.active {
      color: var(--accent);
      background: color-mix(in srgb, var(--accent) 12%, transparent);
      font-weight: 600;
    }
    .sample {
      font-size: 0.8rem;
      color: var(--warn);
      border: 1px solid var(--warn);
      border-radius: 999px;
      padding: 1px 10px;
    }
    main {
      padding: 16px;
      max-width: 1280px;
      margin: 0 auto;
    }
  `,
})
export class App {
  protected readonly nav = NAV;
  protected readonly sampleMode = inject(SAMPLE_MODE);
}

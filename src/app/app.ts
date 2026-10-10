import { ChangeDetectionStrategy, Component } from "@angular/core";
import { RouterLink, RouterLinkActive, RouterOutlet } from "@angular/router";
import { NAV } from "./app.routes";
import { SAMPLE_MODE } from "./core/ipc";
import { FiltersService } from "./core/filters.service";
import { Ipc } from "./core/ipc";
import { LoadingService } from "./core/loading.service";
import { FilterBar } from "./ui/filter-bar";
import { UpdateStatus } from "./ui/update-status";
import { inject } from "@angular/core";

@Component({
  selector: "app-root",
  imports: [RouterOutlet, RouterLink, RouterLinkActive, FilterBar, UpdateStatus],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (loading.busy()) {
      <div class="progress" role="progressbar" aria-label="Reading sessions"></div>
    }
    <header class="top">
      <span class="brand">
        <img src="favicon.svg" alt="" width="24" height="24" />
        <strong>AgentLens</strong>
      </span>
      <nav aria-label="Views">
        @for (item of nav; track item.path) {
          <a [routerLink]="item.path" routerLinkActive="active">{{ item.label }}</a>
        }
      </nav>
      @if (sampleMode) {
        <span class="sample">sample data</span>
      }
      <button type="button" class="refresh" (click)="refresh()" [disabled]="loading.busy()">
        Refresh
      </button>
      @if (!sampleMode) {
        <app-update-status />
      }
    </header>
    <app-filter-bar />
    <main [class.busy]="loading.busy()" [attr.aria-busy]="loading.busy()">
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
      display: inline-flex;
      align-items: center;
      gap: 8px;
      font-size: 1.1rem;
    }
    .brand img {
      display: block;
      border-radius: 6px;
    }
    .progress {
      position: fixed;
      inset: 0 0 auto 0;
      height: 3px;
      z-index: 10;
      overflow: hidden;
      background: color-mix(in srgb, var(--accent) 20%, transparent);
      /* Short loads stay quiet: the bar only appears after a moment. */
      opacity: 0;
      animation: appear 0s 200ms forwards;
    }
    .progress::after {
      content: "";
      display: block;
      width: 35%;
      height: 100%;
      background: var(--accent);
      animation: slide 1.1s ease-in-out infinite;
    }
    @keyframes appear {
      to {
        opacity: 1;
      }
    }
    @keyframes slide {
      from {
        transform: translateX(-100%);
      }
      to {
        transform: translateX(290%);
      }
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
      display: inline-flex;
      align-items: center;
      height: 30px;
      padding: 0 10px;
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
    .refresh {
      background: var(--panel);
      color: var(--text);
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 0 14px;
      cursor: pointer;
    }
    .refresh:disabled {
      opacity: 0.5;
      cursor: default;
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
  protected readonly loading = inject(LoadingService);
  private readonly ipc = inject(Ipc);
  private readonly filters = inject(FiltersService);

  /** Forgets the sessions the backend kept and runs the open view again. */
  protected async refresh(): Promise<void> {
    try {
      await this.ipc.invoke("refresh");
    } finally {
      this.filters.bump();
    }
  }
}

import { ChangeDetectionStrategy, Component, computed, inject } from "@angular/core";
import {
  HelixAppLayout,
  type HelixNavGroup,
  type HelixStatusBarVersion,
  type HelixTopbarAction,
} from "@gravionlabs/helix-shell";
import { NAV, NAV_SECTIONS, type NavItem } from "./app.routes";
import { FiltersService } from "./core/filters.service";
import { Ipc, SAMPLE_MODE } from "./core/ipc";
import { LoadingService } from "./core/loading.service";
import { UpdateService } from "./core/update.service";
import { ZoomService } from "./core/zoom.service";
import { HelixDark } from "./ui/helix-dark";
import { StatusBar } from "./ui/status-bar";

const toNavItem = (item: NavItem) => ({ label: item.label, routerLink: `/${item.path}` });

@Component({
  selector: "app-root",
  imports: [HelixAppLayout, StatusBar],
  hostDirectives: [HelixDark],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { "(document:keydown)": "zoom.handleKey($event)" },
  template: `
    @if (loading.busy()) {
      <div class="progress" role="progressbar" aria-label="Reading sessions"></div>
    }
    <helix-app-layout
      appTitle="AgentLens"
      brandIcon="favicon.svg"
      [navGroups]="navGroups"
      [topbarItems]="[]"
      [topbarActions]="topbarActions()"
      statusBarBrand="AgentLens"
      [statusBarNote]="note"
      [statusBarVersions]="versions()"
    >
      <app-status-bar hxStatusBarEnd />
    </helix-app-layout>
  `,
  styles: `
    .progress {
      position: fixed;
      inset: 0 0 auto 0;
      height: 3px;
      z-index: 1000;
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
  `,
})
export class App {
  protected readonly loading = inject(LoadingService);
  protected readonly zoom = inject(ZoomService);
  private readonly ipc = inject(Ipc);
  private readonly filters = inject(FiltersService);
  private readonly updates = inject(UpdateService);
  private readonly sampleMode = inject(SAMPLE_MODE);

  protected readonly navGroups: HelixNavGroup[] = [
    ...NAV_SECTIONS.map((section) => ({
      section,
      items: NAV.filter((item) => item.group === section).map(toNavItem),
    })),
    { items: NAV.filter((item) => !item.group).map(toNavItem) },
  ];
  /**
   * The topbar action has no disabled state in helix-shell, so while an analysis loads it says
   * "Refreshing…" (and `refresh()` ignores the click) instead of looking clickable.
   */
  protected readonly topbarActions = computed<HelixTopbarAction[]>(() =>
    this.loading.busy()
      ? [{ icon: "pi pi-spin pi-spinner", label: "Refreshing…", command: () => undefined }]
      : [{ icon: "pi pi-refresh", label: "Refresh", command: () => void this.refresh() }],
  );
  protected readonly note = this.sampleMode ? "Nothing here is read from your machine." : undefined;
  protected readonly versions = computed<HelixStatusBarVersion[]>(() => {
    const v = this.updates.version();
    return !this.sampleMode && v ? [{ label: "Version", value: `v${v}` }] : [];
  });

  constructor() {
    void this.updates.loadVersion();
  }

  /** Forgets the sessions the backend kept and runs the open view again. */
  protected async refresh(): Promise<void> {
    if (this.loading.busy()) return;
    try {
      await this.ipc.invoke("refresh");
    } finally {
      this.filters.bump();
    }
  }
}

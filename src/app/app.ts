import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  type TemplateRef,
} from "@angular/core";
import { toSignal } from "@angular/core/rxjs-interop";
import { NavigationEnd, Router } from "@angular/router";
import {
  HelixAppLayout,
  type HelixNavGroup,
  type HelixStatusBarVersion,
  type HelixTopbarActionConfig,
} from "@gravionlabs/helix-shell";
import { HxButton } from "@gravionlabs/helix-ui";
import { filter, map } from "rxjs";
import { NAV, NAV_SECTIONS, type NavItem } from "./app.routes";
import { FiltersService } from "./core/filters.service";
import { Ipc, SAMPLE_MODE } from "./core/ipc";
import { LoadingService } from "./core/loading.service";
import { UpdateService } from "./core/update.service";
import { ZoomService } from "./core/zoom.service";
import { FilterBar } from "./ui/filter-bar";
import { HelixDark } from "./ui/helix-dark";
import { StatusBar } from "./ui/status-bar";

const toNavItem = (item: NavItem) => ({ label: item.label, routerLink: `/${item.path}` });

@Component({
  selector: "app-root",
  imports: [HelixAppLayout, HxButton, FilterBar, StatusBar],
  hostDirectives: [HelixDark],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { "(document:keydown)": "onKeydown($event)" },
  templateUrl: "./app.html",
  styles: `
    .refresh {
      flex: 0 0 auto;
      white-space: nowrap;
    }
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
  private readonly router = inject(Router);

  protected readonly navGroups: HelixNavGroup[] = [
    ...NAV_SECTIONS.map((section) => ({
      section,
      items: NAV.filter((item) => item.group === section).map(toNavItem),
    })),
    { items: NAV.filter((item) => !item.group).map(toNavItem) },
  ];
  private readonly url = toSignal(
    this.router.events.pipe(
      filter((event) => event instanceof NavigationEnd),
      map((event) => event.urlAfterRedirects),
    ),
    { initialValue: this.router.url },
  );
  /** The name of the open view in the page header (a route `title` would also rename the document). */
  protected readonly pageTitle = computed(() => {
    const path = this.url().split(/[?#]/)[0].slice(1);
    return NAV.find((item) => item.path === path)?.label;
  });

  protected readonly note = this.sampleMode ? "Nothing here is read from your machine." : undefined;
  protected readonly versions = computed<HelixStatusBarVersion[]>(() => {
    const v = this.updates.version();
    return !this.sampleMode && v ? [{ label: "Version", value: `v${v}` }] : [];
  });

  constructor() {
    void this.updates.loadVersion();
  }

  private actionList?: HelixTopbarActionConfig[];

  /**
   * The global actions in the page header. Both stay in the `end` zone: an action of the `center`
   * zone moves to another zone below 992px, which would create the filter bar anew and drop what is
   * typed into it. The list is built once, so a load does not rebuild the actions either.
   */
  protected topbarActions(
    filters: TemplateRef<unknown>,
    refresh: TemplateRef<unknown>,
  ): HelixTopbarActionConfig[] {
    return (this.actionList ??= [
      { action: { id: "filters", template: filters } },
      { action: { id: "refresh", template: refresh } },
    ]);
  }

  /** Returns nothing: Angular prevents the default of an event whose listener returns `false`. */
  protected onKeydown(event: KeyboardEvent): void {
    this.zoom.handleKey(event);
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

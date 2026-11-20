import { ChangeDetectionStrategy, Component, computed, inject } from "@angular/core";
import { HxButton, HxTag } from "@gravionlabs/helix-ui";
import { SAMPLE_MODE } from "../core/ipc";
import { TokenFormatService } from "../core/token-format.service";
import { ZoomService } from "../core/zoom.service";
import { CliTools } from "./cli-tools";
import { UpdateStatus } from "./update-status";

/**
 * The end of the status bar of the shell (the version and note are set in `App`): the sample badge, the token format switch (full or
 * rounded counts), the interface size controls, the command line tools and the update check.
 */
@Component({
  selector: "app-status-bar",
  imports: [HxButton, HxTag, CliTools, UpdateStatus],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./status-bar.html",
  styles: `
    :host {
      display: block;
      font-size: 0.85rem;
    }
    .end,
    .zoom {
      display: inline-flex;
      align-items: center;
      gap: 12px;
    }
    .zoom {
      gap: 2px;
    }
    .demo-link {
      color: var(--accent);
      white-space: nowrap;
    }
  `,
})
export class StatusBar {
  protected readonly sampleMode = inject(SAMPLE_MODE);
  protected readonly zoom = inject(ZoomService);
  protected readonly percent = computed(() => Math.round(this.zoom.factor() * 100));

  private readonly tokenFormat = inject(TokenFormatService);
  protected readonly rounded = computed(() => this.tokenFormat.mode() === "rounded");

  protected toggleRounded(): void {
    this.tokenFormat.set(this.rounded() ? "full" : "rounded");
  }
}

import { ChangeDetectionStrategy, Component } from "@angular/core";
import { RouterOutlet } from "@angular/router";

@Component({
  selector: "app-root",
  imports: [RouterOutlet],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <main>
      <h1>AgentLens</h1>
      <p class="muted">What your coding agents do with skills, context and tokens.</p>
      <router-outlet />
    </main>
  `,
})
export class App {}

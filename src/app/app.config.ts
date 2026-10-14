import { ApplicationConfig, provideBrowserGlobalErrorListeners } from "@angular/core";
import { provideRouter, withComponentInputBinding, withHashLocation } from "@angular/router";
import { provideHxTheme } from "@gravionlabs/helix-ui";
import { HX_CHART_LOADER } from "@gravionlabs/helix-ui/chart";
import { routes } from "./app.routes";
import { IPC_FACTORY } from "./core/ipc";
import { Ipc } from "./core/ipc";
import { loadChartJs } from "./ui/chart-loader";

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withHashLocation(), withComponentInputBinding()),
    provideHxTheme(),
    { provide: HX_CHART_LOADER, useValue: loadChartJs },
    { provide: Ipc, useFactory: IPC_FACTORY },
  ],
};

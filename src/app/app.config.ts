import { ApplicationConfig, provideBrowserGlobalErrorListeners } from "@angular/core";
import { provideRouter, withComponentInputBinding, withHashLocation } from "@angular/router";
import { provideHxTheme } from "@gravionlabs/helix-ui";
import { routes } from "./app.routes";
import { IPC_FACTORY } from "./core/ipc";
import { Ipc } from "./core/ipc";

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withHashLocation(), withComponentInputBinding()),
    provideHxTheme(),
    { provide: Ipc, useFactory: IPC_FACTORY },
  ],
};

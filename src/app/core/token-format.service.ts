import { Injectable, signal } from "@angular/core";

const KEY = "agentlens.tokenformat";

export type TokenFormat = "full" | "rounded";

function read(): TokenFormat {
  try {
    return localStorage.getItem(KEY) === "rounded" ? "rounded" : "full";
  } catch {
    return "full";
  }
}

/** The current mode, readable by plain functions such as `tokens()`. */
export const tokenFormat = signal<TokenFormat>(read());

/**
 * How token counts are shown: in full (`2,114,305`) or rounded (`2.11 M`). The choice is
 * remembered between runs when the storage allows it.
 */
@Injectable({ providedIn: "root" })
export class TokenFormatService {
  readonly mode = tokenFormat;

  constructor() {
    this.mode.set(read());
  }

  set(mode: TokenFormat): void {
    this.mode.set(mode);
    try {
      localStorage.setItem(KEY, mode);
    } catch {
      // The storage can be unavailable; the choice then lasts until the app closes.
    }
  }
}

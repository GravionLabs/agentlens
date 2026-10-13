import { beforeEach, describe, expect, it, vi } from "vitest";
import { TokenFormatService } from "./token-format.service";

describe("TokenFormatService", () => {
  let store: Map<string, string>;
  beforeEach(() => {
    store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    });
  });

  it("defaults to full", () => {
    expect(new TokenFormatService().mode()).toBe("full");
  });

  it("remembers the mode", () => {
    new TokenFormatService().set("rounded");
    expect(store.get("agentlens.tokenformat")).toBe("rounded");
    expect(new TokenFormatService().mode()).toBe("rounded");
  });

  it("ignores an unknown stored value", () => {
    store.set("agentlens.tokenformat", "bogus");
    expect(new TokenFormatService().mode()).toBe("full");
  });

  it("works when the storage is unavailable", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    });
    const service = new TokenFormatService();
    expect(service.mode()).toBe("full");
    service.set("rounded");
    expect(service.mode()).toBe("rounded");
  });
});

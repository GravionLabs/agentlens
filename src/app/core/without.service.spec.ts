import { beforeEach, describe, expect, it } from "vitest";
import { WithoutService } from "./without.service";

describe("WithoutService", () => {
  let service: WithoutService;
  beforeEach(() => {
    service = new WithoutService();
  });

  it("starts empty", () => {
    expect(service.items()).toEqual([]);
    expect(service.has("skill", "demo")).toBe(false);
  });

  it("adds KIND:NAME in the order of adding", () => {
    service.add("skill", "demo");
    service.add("mcp-server", "github");
    expect(service.items()).toEqual(["skill:demo", "mcp-server:github"]);
    expect(service.has("skill", "demo")).toBe(true);
    expect(service.has("agent", "demo")).toBe(false);
  });

  it("ignores an item that is already on the list", () => {
    service.add("skill", "demo");
    service.add("skill", "demo");
    expect(service.items()).toEqual(["skill:demo"]);
  });

  it("removes one item and leaves the others", () => {
    service.add("skill", "a");
    service.add("agent", "b");
    service.remove("skill:a");
    expect(service.items()).toEqual(["agent:b"]);
    service.remove("skill:missing");
    expect(service.items()).toEqual(["agent:b"]);
  });

  it("is provided in root", () => {
    expect((WithoutService as unknown as { ɵprov: { providedIn: string } }).ɵprov.providedIn).toBe(
      "root",
    );
  });
});

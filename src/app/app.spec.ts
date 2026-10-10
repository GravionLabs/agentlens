import { describe, expect, it } from "vitest";
import { routes } from "./app.routes";

describe("app", () => {
  it("has a router configuration", () => {
    expect(Array.isArray(routes)).toBe(true);
  });
});

import { describe, expect, it } from "vitest";
import { MAX_SAMPLE, hxEnabled, hxSamplePoints } from "./hx-flag";

describe("hx switch", () => {
  it("is off only when the parameter is absent (the SVG stays the default)", () => {
    expect(hxEnabled(undefined)).toBe(false);
    for (const v of ["", "abc", "0", "400"]) expect(hxEnabled(v)).toBe(true);
  });

  it("takes a sample size only from a positive whole number", () => {
    expect(hxSamplePoints("400")).toBe(400);
    for (const v of [undefined, "", "abc", "0", "-5", "1.5", "4e2", " 4"]) {
      expect(hxSamplePoints(v)).toBeNull();
    }
  });

  it("caps the sample size", () => {
    expect(hxSamplePoints("5000")).toBe(MAX_SAMPLE);
    expect(hxSamplePoints("99999999")).toBe(MAX_SAMPLE);
  });
});

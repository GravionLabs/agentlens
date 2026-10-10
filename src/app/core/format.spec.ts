import { describe, expect, it } from "vitest";
import {
  compact,
  day,
  minutes,
  number,
  percent,
  shortId,
  sourceLabel,
  tokens,
  usd,
} from "./format";

describe("format", () => {
  it("groups thousands and marks missing values as n/a", () => {
    expect(number(1234567)).toBe("1,234,567");
    expect(number(null)).toBe("n/a");
    expect(percent(null)).toBe("n/a");
    expect(minutes(undefined)).toBe("n/a");
  });

  it("marks estimates with a tilde", () => {
    expect(tokens(1500, true)).toBe("~1,500");
    expect(tokens(1500)).toBe("1,500");
    expect(usd(1603.954, true)).toBe("~$1,603.95");
    expect(usd(0.5)).toBe("$0.50");
  });

  it("shortens numbers for chart labels", () => {
    expect(compact(999)).toBe("999");
    expect(compact(1500)).toBe("1.5k");
    expect(compact(12_345)).toBe("12k");
    expect(compact(2_400_000)).toBe("2.4M");
  });

  it("formats rates, durations, days and ids", () => {
    expect(percent(0.3460207)).toBe("34.6%");
    expect(minutes(90_000)).toBe("1.5");
    expect(day("2026-10-02T09:00:00Z")).toBe("2026-10-02");
    expect(day(null)).toBe("-");
    expect(shortId("0123456789")).toBe("01234567");
    expect(sourceLabel("vscode-copilot")).toBe("VS Code Copilot");
  });
});

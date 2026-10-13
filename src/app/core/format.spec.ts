import { afterEach, describe, expect, it } from "vitest";
import {
  compact,
  day,
  timeOfDay,
  minutes,
  number,
  pathLabel,
  percent,
  shortId,
  shortPath,
  sourceLabel,
  tokens,
  usd,
} from "./format";
import { tokenFormat } from "./token-format.service";

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
    expect(timeOfDay("2026-10-02T09:00:20.123Z")).toBe("09:00:20");
    expect(timeOfDay(null)).toBe("-");
    expect(shortId("0123456789")).toBe("01234567");
    expect(sourceLabel("vscode-copilot")).toBe("VS Code Copilot");
  });

  it("shortens paths to their last two segments and leaves other values alone", () => {
    expect(shortPath("/home/u/Source/gravionlabs/ariadne")).toBe("gravionlabs/ariadne");
    expect(shortPath("C:\\Users\\u\\repo\\app")).toBe("repo/app");
    expect(shortPath("/work")).toBe("/work");
    expect(shortPath(null)).toBe("-");
    expect(pathLabel("/home/u/a/b")).toBe("a/b");
    expect(pathLabel("claude-sonnet-5-5")).toBe("claude-sonnet-5-5");
    expect(pathLabel("#327")).toBe("#327");
  });
});

describe("tokens in rounded mode", () => {
  afterEach(() => tokenFormat.set("full"));

  it("rounds to two decimals with a unit", () => {
    tokenFormat.set("rounded");
    expect(tokens(999)).toBe("999");
    expect(tokens(1000)).toBe("1.00 k");
    expect(tokens(2114)).toBe("2.11 k");
    expect(tokens(999_999)).toBe("1.00 M");
    expect(tokens(1_000_000)).toBe("1.00 M");
    expect(tokens(1_549_999)).toBe("1.55 M");
    expect(tokens(3_200_000_000)).toBe("3.20 B");
    expect(tokens(999_999_999)).toBe("1.00 B");
  });

  it("keeps sign, zero, estimates and missing values", () => {
    tokenFormat.set("rounded");
    expect(tokens(0)).toBe("0");
    expect(tokens(-1_550_000)).toBe("-1.55 M");
    expect(tokens(-5)).toBe("-5");
    expect(tokens(1_550_000, true)).toBe("~1.55 M");
    expect(tokens(null)).toBe("n/a");
    expect(tokens(undefined, true)).toBe("n/a");
  });

  it("leaves full mode unchanged", () => {
    expect(tokens(1_550_000)).toBe("1,550,000");
  });
});

import { afterEach, describe, expect, it } from "vitest";
import { tokenFormat } from "../../core/token-format.service";
import type { ContextKind } from "../../core/types";
import type { ContextRow } from "../../core/types";
import { byOwner, firstRequestNote, inView } from "./context";

describe("context views", () => {
  const kinds: ContextKind[] = [
    "tool-listing",
    "tool-definition",
    "mcp-instructions",
    "agent-listing",
    "skill-listing",
    "system-prompt",
    "instruction-file",
  ];

  it("groups the kinds like `agentlens context --kind`", () => {
    expect(kinds.filter((k) => inView(k, "tools"))).toEqual([
      "tool-listing",
      "tool-definition",
      "mcp-instructions",
    ]);
    expect(kinds.filter((k) => inView(k, "instructions"))).toEqual(["instruction-file"]);
    expect(kinds.filter((k) => inView(k, "agents"))).toEqual(["agent-listing"]);
    expect(kinds.filter((k) => inView(k, "skills"))).toEqual(["skill-listing"]);
    expect(kinds.filter((k) => inView(k, "all"))).toEqual(kinds);
  });
});

describe("context grouping", () => {
  const row = (project: string, tokens: number, requests: number): ContextRow => ({
    source: "claude-code",
    project,
    kind: "mcp-instructions",
    owner: "srv",
    items: 1,
    bytes: 10,
    tokens: { count: tokens, measured: false },
    requests,
    overhead_tokens: { count: tokens * requests, measured: false },
  });

  it("sums the projects of one owner into a single row", () => {
    const rows = byOwner([
      row("/a", 10, 2),
      row("/b", 5, 4),
      { ...row("/a", 1, 1), owner: "other" },
    ]);
    expect(rows).toHaveLength(2);
    const srv = rows.find((r) => r.owner === "srv")!;
    expect(srv).toMatchObject({
      items: 2,
      bytes: 20,
      tokens: { count: 15, measured: false },
      requests: 6,
      overhead_tokens: { count: 40, measured: false },
    });
    expect(srv.project).toBeNull();
  });

  it("keeps a sum an estimate unless every part was measured", () => {
    const measured = { ...row("/a", 10, 2), tokens: { count: 10, measured: true } };
    const both = byOwner([measured, { ...measured, project: "/b" }]);
    expect(both[0].tokens).toEqual({ count: 20, measured: true });
    const mixed = byOwner([measured, row("/b", 5, 4)]);
    expect(mixed[0].tokens).toEqual({ count: 15, measured: false });
  });
});

describe("first request note", () => {
  const f = { sessions: 2, total_tokens: 4_228_610, max_tokens: 2_500_000 };
  afterEach(() => tokenFormat.set("full"));

  it("shows full counts without a title by default", () => {
    const n = firstRequestNote("claude-code", f);
    expect(n.text).toContain("holds 2,114,305 tokens on average, at most 2,500,000 (");
    expect(n.title).toBeNull();
  });

  it("rounds the counts and keeps the exact ones in the title", () => {
    tokenFormat.set("rounded");
    const n = firstRequestNote("claude-code", f);
    expect(n.text).toContain("holds 2.11 M tokens on average, at most 2.50 M (");
    expect(n.title).toBe("Average 2,114,305 tokens, at most 2,500,000");
  });
});

import { describe, expect, it } from "vitest";
import type { ContextKind } from "../../core/types";
import { inView } from "./context";

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

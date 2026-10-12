import { describe, expect, it } from "vitest";
import type { UnusedItem } from "../../core/types";
import { referencedBy } from "./unused";

const item = (over: Partial<UnusedItem>): UnusedItem => ({
  kind: "skill",
  name: "verify-project",
  project: null,
  clients: ["claude-code"],
  uses: 0,
  requests: 10,
  overhead_tokens: { count: 100, measured: false },
  ...over,
});

describe("referencedBy", () => {
  it("names the agents, commands and skills that mention the skill", () => {
    expect(
      referencedBy(
        item({
          referenced_by: [
            { kind: "agent", name: "dev-workflow" },
            { kind: "command", name: "ship" },
          ],
        }),
      ),
    ).toBe("agent dev-workflow, command ship");
  });

  it("is empty when nothing names the skill", () => {
    expect(referencedBy(item({}))).toBe("");
    expect(referencedBy(item({ referenced_by: [] }))).toBe("");
  });
});

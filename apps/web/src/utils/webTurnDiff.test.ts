import { describe, expect, it } from "vitest";
import { parseWebTurnDiff } from "./webTurnDiff";

describe("parseWebTurnDiff", () => {
  it("summarizes files and changed lines from a unified diff", () => {
    const parsed = parseWebTurnDiff([
      "diff --git a/src/a.ts b/src/a.ts",
      "--- a/src/a.ts",
      "+++ b/src/a.ts",
      "@@ -1 +1,2 @@",
      "-old",
      "+new",
      "+next",
      "diff --git a/src/b.ts b/src/b.ts",
      "--- a/src/b.ts",
      "+++ b/src/b.ts",
      "+added",
    ].join("\n"));

    expect(parsed.title).toBe("2 files changed · +3 −1");
    expect(parsed.fileCount).toBe(2);
    expect(parsed.additions).toBe(3);
    expect(parsed.deletions).toBe(1);
  });
});

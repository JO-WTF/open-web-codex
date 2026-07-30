import assert from "node:assert/strict";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, it } from "node:test";

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));
const setupScript = join(repoRoot, "scripts", "setup-maps-mcp-env.sh");

describe("maps MCP environment location", () => {
  it("rejects a missing virtualenv target inside the Plugin root before creating it", () => {
    const testRoot = mkdtempSync(join(tmpdir(), "open-web-codex-maps-env-"));
    const candidate = join(
      repoRoot,
      "tools",
      "maps-mcp",
      `.contract-test-venv-${basename(testRoot)}`,
    );
    assert.equal(existsSync(candidate), false, "test target must start absent");

    try {
      const result = spawnSync(setupScript, [], {
        cwd: repoRoot,
        encoding: "utf8",
        env: {
          ...process.env,
          OPEN_WEB_CODEX_LOG_DIR: join(testRoot, "logs"),
          OPEN_WEB_CODEX_MAPS_MCP_VENV: candidate,
        },
      });

      assert.equal(result.error, undefined);
      assert.equal(result.status, 2, result.stderr);
      assert.match(
        result.stderr,
        /unsupported maps MCP virtualenv location inside Plugin root/,
      );
      assert.equal(existsSync(candidate), false);
    } finally {
      rmSync(candidate, { recursive: true, force: true });
      rmSync(testRoot, { recursive: true, force: true });
    }
  });
});

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execSync } from "node:child_process";

const WEB_ROOT = resolve(import.meta.dirname, "..");
const DEFAULT_BIND = process.env.PLATFORM_SMOKE_BIND ?? "127.0.0.1:4811";
const DATABASE_URL = (() => {
  if (process.env.DATABASE_URL) {
    return process.env.DATABASE_URL;
  }
  const user = process.env.USER ?? "postgres";
  const password = process.env.PLATFORM_SMOKE_DB_PASSWORD;
  const credentials = password
    ? `${user}:${encodeURIComponent(password)}`
    : user;
  return `postgres://${credentials}@localhost:5432/open_web_codex_batch23_smoke`;
})();

function sleep(ms) {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, ms));
}

async function fetchJson(baseUrl, path, init = {}) {
  const response = await fetch(`${baseUrl}${path}`, {
    ...init,
    headers: {
      "content-type": "application/json",
      ...init.headers,
    },
  });
  const text = await response.text();
  let body = null;
  if (text) {
    body = JSON.parse(text);
  }
  return { status: response.status, body };
}

async function waitForHealth(baseUrl, timeoutMs = 30_000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const { status, body } = await fetchJson(baseUrl, "/api/health");
      if (status === 200 && body?.ok) {
        return;
      }
    } catch {
      // retry until timeout
    }
    await sleep(250);
  }
  throw new Error(`platform server did not become healthy at ${baseUrl}`);
}

async function waitForApproval(baseUrl, token, runId, timeoutMs = 10_000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    const { status, body } = await fetchJson(
      baseUrl,
      `/api/approvals?run_id=${encodeURIComponent(runId)}`,
      { headers: { authorization: `Bearer ${token}` } },
    );
    if (status === 200 && Array.isArray(body) && body.length > 0) {
      return body[0];
    }
    await sleep(200);
  }
  throw new Error(`timed out waiting for approval on run ${runId}`);
}

function createBareRepo() {
  const dir = mkdtempSync(join(tmpdir(), "owc-batch23-repo-"));
  execSync("git init --bare", { cwd: dir, stdio: "ignore" });
  const work = mkdtempSync(join(tmpdir(), "owc-batch23-work-"));
  execSync(`git clone ${dir} ${work}`, { stdio: "ignore" });
  execSync('git config user.email "smoke@example.com"', { cwd: work, stdio: "ignore" });
  execSync('git config user.name "Smoke"', { cwd: work, stdio: "ignore" });
  writeFileSync(join(work, "README.md"), "# smoke\n");
  execSync("git add README.md", { cwd: work, stdio: "ignore" });
  execSync('git commit -m "init"', { cwd: work, stdio: "ignore" });
  execSync("git branch -M main", { cwd: work, stdio: "ignore" });
  execSync(`git push origin main`, { cwd: work, stdio: "ignore" });
  rmSync(work, { recursive: true, force: true });
  return { dir, url: `file://${dir}` };
}

async function main() {
  const dataRoot = mkdtempSync(join(tmpdir(), "owc-batch23-data-"));
  const repo = createBareRepo();
  const baseUrl = `http://${DEFAULT_BIND}`;

  try {
    const dbName = new URL(DATABASE_URL).pathname.slice(1);
    execSync(`dropdb --if-exists ${dbName}`, { stdio: "ignore" });
    execSync(`createdb ${dbName}`, { stdio: "ignore" });
  } catch {
    // createdb/dropdb may be unavailable; migrations will fail loudly if DB is missing.
  }

  const server = spawn(
    "cargo",
    [
      "run",
      "--quiet",
      "--manifest-path",
      join(WEB_ROOT, "Cargo.toml"),
      "-p",
      "open-web-codex-server",
      "--",
      "--bind",
      DEFAULT_BIND,
      "--codex-mode",
      "fake",
    ],
    {
      cwd: WEB_ROOT,
      env: {
        ...process.env,
        DATABASE_URL,
        DATA_ROOT: dataRoot,
        RUSTUP_TOOLCHAIN: process.env.RUSTUP_TOOLCHAIN ?? "1.95.0",
      },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );

  let serverLog = "";
  server.stdout.on("data", (chunk) => {
    serverLog += chunk.toString();
  });
  server.stderr.on("data", (chunk) => {
    serverLog += chunk.toString();
  });

  const shutdown = () => {
    if (!server.killed) {
      server.kill("SIGTERM");
    }
  };
  process.on("exit", shutdown);
  process.on("SIGINT", () => {
    shutdown();
    process.exit(1);
  });

  try {
    await waitForHealth(baseUrl);

    const bootstrap = await fetchJson(baseUrl, "/api/bootstrap", {
      method: "POST",
      body: JSON.stringify({
        name: "Smoke Owner",
        email: `smoke-${Date.now()}@example.com`,
        password: "smoke-password",
      }),
    });
    if (bootstrap.status !== 200) {
      throw new Error(`bootstrap failed: ${bootstrap.status} ${JSON.stringify(bootstrap.body)}`);
    }
    const token = bootstrap.body.session_token;

    const project = await fetchJson(baseUrl, "/api/projects", {
      method: "POST",
      headers: { authorization: `Bearer ${token}` },
      body: JSON.stringify({
        name: "Smoke Project",
        git_url: repo.url,
        default_branch: "main",
      }),
    });
    if (project.status !== 200) {
      throw new Error(`create project failed: ${project.status}`);
    }

    const task = await fetchJson(baseUrl, "/api/tasks", {
      method: "POST",
      headers: { authorization: `Bearer ${token}` },
      body: JSON.stringify({
        project_id: project.body.id,
        title: "Smoke Task",
      }),
    });
    if (task.status !== 200) {
      throw new Error(`create task failed: ${task.status}`);
    }

    const idempotencyKey = `smoke-run-${Date.now()}`;
    const startHeaders = {
      authorization: `Bearer ${token}`,
      "idempotency-key": idempotencyKey,
    };
    const firstRun = await fetchJson(
      baseUrl,
      `/api/tasks/${encodeURIComponent(task.body.id)}/runs`,
      { method: "POST", headers: startHeaders, body: "{}" },
    );
    if (firstRun.status !== 200) {
      throw new Error(`start run failed: ${firstRun.status} ${JSON.stringify(firstRun.body)}`);
    }

    const secondRun = await fetchJson(
      baseUrl,
      `/api/tasks/${encodeURIComponent(task.body.id)}/runs`,
      { method: "POST", headers: startHeaders, body: "{}" },
    );
    if (secondRun.status !== 200) {
      throw new Error(`idempotent start run failed: ${secondRun.status}`);
    }
    if (firstRun.body.run.id !== secondRun.body.run.id) {
      throw new Error("idempotency key did not return the same run");
    }

    const runId = firstRun.body.run.id;
    const approval = await waitForApproval(baseUrl, token, runId);
    if (approval.status !== "pending") {
      throw new Error(`expected pending approval, got ${approval.status}`);
    }

    const decision = await fetchJson(
      baseUrl,
      `/api/approvals/${encodeURIComponent(approval.id)}/decision`,
      {
        method: "POST",
        headers: { authorization: `Bearer ${token}` },
        body: JSON.stringify({ decision: "approved" }),
      },
    );
    if (decision.status !== 200) {
      throw new Error(`approval decision failed: ${decision.status}`);
    }

    const duplicateDecision = await fetchJson(
      baseUrl,
      `/api/approvals/${encodeURIComponent(approval.id)}/decision`,
      {
        method: "POST",
        headers: { authorization: `Bearer ${token}` },
        body: JSON.stringify({ decision: "rejected" }),
      },
    );
    if (duplicateDecision.status !== 409) {
      throw new Error(`expected CAS conflict, got ${duplicateDecision.status}`);
    }

    const profileRow = await fetchJson(baseUrl, "/api/me", {
      headers: { authorization: `Bearer ${token}` },
    });
    if (profileRow.status !== 200) {
      throw new Error(`me failed: ${profileRow.status}`);
    }

    console.log(
      JSON.stringify(
        {
          ok: true,
          runId,
          approvalId: approval.id,
          threadId: firstRun.body.run.codex_thread_id,
          idempotent: true,
        },
        null,
        2,
      ),
    );
  } finally {
    shutdown();
    rmSync(dataRoot, { recursive: true, force: true });
    rmSync(repo.dir, { recursive: true, force: true });
    if (server.exitCode == null) {
      await sleep(500);
    }
    if (server.exitCode != null && server.exitCode !== 0 && !serverLog.includes("listening")) {
      console.error(serverLog);
    }
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});

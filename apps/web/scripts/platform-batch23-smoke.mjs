import { createHash, randomUUID } from "node:crypto";
import { spawn, execSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

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

function sha256Hex(value) {
  return createHash("sha256").update(value).digest("hex");
}

function psqlJson(sql) {
  const oneLine = sql.replace(/\s+/g, " ").trim();
  const output = execSync(`psql "${DATABASE_URL}" -v ON_ERROR_STOP=1 -At -c ${JSON.stringify(oneLine)}`, {
    encoding: "utf8",
  }).trim();
  return output;
}

function psqlScalar(sql) {
  return psqlJson(sql);
}

async function fetchJson(baseUrl, path, init = {}) {
  const response = await fetch(`${baseUrl}${path}`, {
    ...init,
    headers: {
      ...(init.body ? { "content-type": "application/json" } : {}),
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

function assertStatus(label, actual, expected) {
  if (actual !== expected) {
    throw new Error(`${label}: expected HTTP ${expected}, got ${actual}`);
  }
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
  execSync("git push origin main", { cwd: work, stdio: "ignore" });
  rmSync(work, { recursive: true, force: true });
  return { dir, url: `file://${dir}` };
}

function createIsolatedUser(email, password, orgSlug) {
  const passwordHash = sha256Hex(password);
  psqlJson(`
    WITH new_user AS (
      INSERT INTO users (name, email, password_hash, role)
      VALUES ('Intruder', '${email}', '${passwordHash}', 'member')
      RETURNING id
    ), new_org AS (
      INSERT INTO organizations (name, slug)
      VALUES ('Intruder Org', '${orgSlug}')
      RETURNING id
    )
    INSERT INTO memberships (organization_id, user_id, role)
    SELECT new_org.id, new_user.id, 'owner'
    FROM new_org, new_user;
  `);
}

async function login(baseUrl, email, password) {
  const response = await fetchJson(baseUrl, "/api/sessions", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });
  assertStatus("login", response.status, 200);
  return response.body.session_token;
}

async function main() {
  const dataRoot = mkdtempSync(join(tmpdir(), "owc-batch23-data-"));
  const repo = createBareRepo();
  const baseUrl = `http://${DEFAULT_BIND}`;

  const dbName = new URL(DATABASE_URL).pathname.slice(1);
  execSync(`dropdb --if-exists ${dbName}`, { stdio: "ignore" });
  execSync(`createdb ${dbName}`, { stdio: "ignore" });

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

    const unauth = await fetchJson(baseUrl, "/api/tasks");
    assertStatus("unauthenticated tasks", unauth.status, 401);

    const bootstrap = await fetchJson(baseUrl, "/api/bootstrap", {
      method: "POST",
      body: JSON.stringify({
        name: "Smoke Owner",
        email: `owner-${Date.now()}@example.com`,
        password: "smoke-password",
      }),
    });
    assertStatus("bootstrap", bootstrap.status, 200);
    const ownerToken = bootstrap.body.session_token;
    const organizationId = bootstrap.body.organization.id;

    const duplicateBootstrap = await fetchJson(baseUrl, "/api/bootstrap", {
      method: "POST",
      body: JSON.stringify({
        name: "Another Owner",
        email: `other-${Date.now()}@example.com`,
        password: "smoke-password",
      }),
    });
    assertStatus("duplicate bootstrap", duplicateBootstrap.status, 409);

    const project = await fetchJson(baseUrl, "/api/projects", {
      method: "POST",
      headers: { authorization: `Bearer ${ownerToken}` },
      body: JSON.stringify({
        name: "Smoke Project",
        git_url: repo.url,
        default_branch: "main",
        organization_id: organizationId,
      }),
    });
    assertStatus("create project", project.status, 200);
    if (project.body.organization_id !== organizationId) {
      throw new Error("project is not scoped to bootstrap organization");
    }

    const task = await fetchJson(baseUrl, "/api/tasks", {
      method: "POST",
      headers: { authorization: `Bearer ${ownerToken}` },
      body: JSON.stringify({
        project_id: project.body.id,
        title: "Smoke Task",
      }),
    });
    assertStatus("create task", task.status, 200);

    const intruderEmail = `intruder-${Date.now()}@example.com`;
    createIsolatedUser(intruderEmail, "intruder-password", `intruder-${randomUUID().slice(0, 8)}`);
    const intruderToken = await login(baseUrl, intruderEmail, "intruder-password");

    const forbiddenTask = await fetchJson(baseUrl, `/api/tasks/${encodeURIComponent(task.body.id)}`, {
      headers: { authorization: `Bearer ${intruderToken}` },
    });
    assertStatus("cross-org task access", forbiddenTask.status, 404);

    const idempotencyKey = `smoke-run-${Date.now()}`;
    const startHeaders = {
      authorization: `Bearer ${ownerToken}`,
      "idempotency-key": idempotencyKey,
    };
    const firstRun = await fetchJson(
      baseUrl,
      `/api/tasks/${encodeURIComponent(task.body.id)}/runs`,
      { method: "POST", headers: startHeaders, body: "{}" },
    );
    assertStatus("start run", firstRun.status, 200);
    if (firstRun.body.run.status !== "running") {
      throw new Error(`expected running run, got ${firstRun.body.run.status}`);
    }
    if (!firstRun.body.run.codex_thread_id) {
      throw new Error("run is missing codex_thread_id");
    }

    const secondRun = await fetchJson(
      baseUrl,
      `/api/tasks/${encodeURIComponent(task.body.id)}/runs`,
      { method: "POST", headers: startHeaders, body: "{}" },
    );
    assertStatus("idempotent start run", secondRun.status, 200);
    if (firstRun.body.run.id !== secondRun.body.run.id) {
      throw new Error("idempotency key did not return the same run");
    }

    const runCount = Number(
      psqlScalar(`SELECT COUNT(*) FROM runs WHERE task_id = '${task.body.id}';`),
    );
    if (runCount !== 1) {
      throw new Error(`expected exactly one run row, found ${runCount}`);
    }

    const workspaceState = psqlScalar(
      `SELECT state FROM run_workspaces WHERE run_id = '${firstRun.body.run.id}';`,
    );
    if (workspaceState !== "ready") {
      throw new Error(`expected run workspace state ready, got ${workspaceState}`);
    }

    const profileCount = Number(
      psqlScalar(`SELECT COUNT(*) FROM profiles WHERE user_id = '${bootstrap.body.user.id}';`),
    );
    if (profileCount !== 1) {
      throw new Error(`expected one profile row for owner, found ${profileCount}`);
    }

    const forbiddenRun = await fetchJson(
      baseUrl,
      `/api/runs/${encodeURIComponent(firstRun.body.run.id)}`,
      { headers: { authorization: `Bearer ${intruderToken}` } },
    );
    assertStatus("cross-org run access", forbiddenRun.status, 404);

    const approval = await waitForApproval(baseUrl, ownerToken, firstRun.body.run.id);
    if (approval.status !== "pending") {
      throw new Error(`expected pending approval, got ${approval.status}`);
    }
    if (!approval.codex_request_id || !approval.workspace_id) {
      throw new Error("approval is missing codex_request_id or workspace_id");
    }

    const forbiddenApprovalList = await fetchJson(
      baseUrl,
      `/api/approvals?run_id=${encodeURIComponent(firstRun.body.run.id)}`,
      { headers: { authorization: `Bearer ${intruderToken}` } },
    );
    assertStatus("cross-org approval list", forbiddenApprovalList.status, 404);

    const decision = await fetchJson(
      baseUrl,
      `/api/approvals/${encodeURIComponent(approval.id)}/decision`,
      {
        method: "POST",
        headers: { authorization: `Bearer ${ownerToken}` },
        body: JSON.stringify({ decision: "approved" }),
      },
    );
    assertStatus("approval decision", decision.status, 200);

    const duplicateDecision = await fetchJson(
      baseUrl,
      `/api/approvals/${encodeURIComponent(approval.id)}/decision`,
      {
        method: "POST",
        headers: { authorization: `Bearer ${ownerToken}` },
        body: JSON.stringify({ decision: "rejected" }),
      },
    );
    assertStatus("approval CAS conflict", duplicateDecision.status, 409);

    const forbiddenDecision = await fetchJson(
      baseUrl,
      `/api/approvals/${encodeURIComponent(approval.id)}/decision`,
      {
        method: "POST",
        headers: { authorization: `Bearer ${intruderToken}` },
        body: JSON.stringify({ decision: "approved" }),
      },
    );
    assertStatus("cross-org approval decision", forbiddenDecision.status, 404);

    const approvalStatus = psqlScalar(
      `SELECT status FROM approvals WHERE id = '${approval.id}';`,
    );
    if (approvalStatus !== "approved") {
      throw new Error(`expected approved status in database, got ${approvalStatus}`);
    }

    const runStatus = psqlScalar(`SELECT status FROM runs WHERE id = '${firstRun.body.run.id}';`);
    if (runStatus !== "running") {
      throw new Error(`expected run status running after approval, got ${runStatus}`);
    }

    const badGitProject = await fetchJson(baseUrl, "/api/projects", {
      method: "POST",
      headers: { authorization: `Bearer ${ownerToken}` },
      body: JSON.stringify({
        name: "Bad Git Project",
        git_url: "javascript:alert(1)",
        default_branch: "main",
        organization_id: organizationId,
      }),
    });
    assertStatus("reject unsafe git url", badGitProject.status, 400);

    console.log(
      JSON.stringify(
        {
          ok: true,
          checks: [
            "bootstrap",
            "org_scoped_project",
            "cross_org_denial",
            "idempotent_single_run",
            "workspace_ready",
            "profile_row",
            "approval_persisted",
            "approval_cas",
            "unsafe_git_rejected",
          ],
          runId: firstRun.body.run.id,
          approvalId: approval.id,
        },
        null,
        2,
      ),
    );
  } finally {
    shutdown();
    rmSync(dataRoot, { recursive: true, force: true });
    rmSync(repo.dir, { recursive: true, force: true });
    await sleep(500);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});

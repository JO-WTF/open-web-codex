#!/usr/bin/env node
import { createHash, randomUUID } from "node:crypto";
import { spawn, execSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const WEB_ROOT = resolve(import.meta.dirname, "..");
const DEFAULT_BIND = process.env.PLATFORM_WEB_SMOKE_BIND ?? "127.0.0.1:4812";
const DATABASE_URL = (() => {
  if (process.env.DATABASE_URL) {
    return process.env.DATABASE_URL;
  }
  const user = process.env.USER ?? "postgres";
  const password = process.env.PLATFORM_SMOKE_DB_PASSWORD;
  const credentials = password
    ? `${user}:${encodeURIComponent(password)}`
    : user;
  return `postgres://${credentials}@localhost:5432/open_web_codex_web_smoke`;
})();

function sleep(ms) {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, ms));
}

function sha256Hex(value) {
  return createHash("sha256").update(value).digest("hex");
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
      // retry
    }
    await sleep(250);
  }
  throw new Error(`platform server did not become healthy at ${baseUrl}`);
}

function createBareRepo() {
  const dir = mkdtempSync(join(tmpdir(), "owc-web-repo-"));
  execSync("git init --bare", { cwd: dir, stdio: "ignore" });
  const work = mkdtempSync(join(tmpdir(), "owc-web-work-"));
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

async function main() {
  const dataRoot = mkdtempSync(join(tmpdir(), "owc-web-data-"));
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

  try {
    await waitForHealth(baseUrl);

    const legacyRpc = await fetchJson(baseUrl, "/api/rpc", {
      method: "POST",
      body: JSON.stringify({ method: "list_workspaces", params: {} }),
    });
    assertStatus("legacy rpc removed", legacyRpc.status, 404);

    const legacyEvents = await fetchJson(baseUrl, "/api/events");
    assertStatus("legacy events removed", legacyEvents.status, 404);

    const bootstrap = await fetchJson(baseUrl, "/api/bootstrap", {
      method: "POST",
      body: JSON.stringify({
        name: "Web Smoke",
        email: `web-${Date.now()}@example.com`,
        password: "smoke-password",
      }),
    });
    assertStatus("bootstrap", bootstrap.status, 200);
    const token = bootstrap.body.session_token;

    const project = await fetchJson(baseUrl, "/api/projects", {
      method: "POST",
      headers: { authorization: `Bearer ${token}` },
      body: JSON.stringify({
        name: "Web Project",
        git_url: repo.url,
        default_branch: "main",
      }),
    });
    assertStatus("create project", project.status, 200);

    const disposable = await fetchJson(baseUrl, "/api/projects", {
      method: "POST",
      headers: { authorization: `Bearer ${token}` },
      body: JSON.stringify({
        name: "Disposable",
        git_url: repo.url,
        default_branch: "main",
      }),
    });
    assertStatus("create disposable project", disposable.status, 200);

    const deleted = await fetchJson(
      baseUrl,
      `/api/projects/${encodeURIComponent(disposable.body.id)}`,
      { method: "DELETE", headers: { authorization: `Bearer ${token}` } },
    );
    assertStatus("delete project", deleted.status, 200);

    const task = await fetchJson(baseUrl, "/api/tasks", {
      method: "POST",
      headers: { authorization: `Bearer ${token}` },
      body: JSON.stringify({ project_id: project.body.id, title: "Web Task" }),
    });
    assertStatus("create task", task.status, 200);

    const run = await fetchJson(
      baseUrl,
      `/api/tasks/${encodeURIComponent(task.body.id)}/runs`,
      { method: "POST", headers: { authorization: `Bearer ${token}` }, body: "{}" },
    );
    assertStatus("start run", run.status, 200);

    const active = await fetchJson(
      baseUrl,
      `/api/tasks/${encodeURIComponent(task.body.id)}/active-run`,
      { headers: { authorization: `Bearer ${token}` } },
    );
    assertStatus("active run", active.status, 200);
    if (!active.body.run || active.body.run.id !== run.body.run.id) {
      throw new Error("active-run did not return the started run");
    }

    const files = await fetchJson(
      baseUrl,
      `/api/runs/${encodeURIComponent(run.body.run.id)}/files`,
      { headers: { authorization: `Bearer ${token}` } },
    );
    assertStatus("list run files", files.status, 200);
    if (!Array.isArray(files.body.files) || !files.body.files.includes("README.md")) {
      throw new Error(`expected README.md in run files, got ${JSON.stringify(files.body.files)}`);
    }

    const content = await fetchJson(
      baseUrl,
      `/api/runs/${encodeURIComponent(run.body.run.id)}/files/content?path=${encodeURIComponent("README.md")}`,
      { headers: { authorization: `Bearer ${token}` } },
    );
    assertStatus("read run file", content.status, 200);
    if (!String(content.body.content).includes("# smoke")) {
      throw new Error("run file content did not match cloned README");
    }

    const gitStatus = await fetchJson(
      baseUrl,
      `/api/runs/${encodeURIComponent(run.body.run.id)}/git-status`,
      { headers: { authorization: `Bearer ${token}` } },
    );
    assertStatus("git status", gitStatus.status, 200);
    if (!Array.isArray(gitStatus.body.files)) {
      throw new Error("git status response missing files array");
    }

    const message = await fetchJson(
      baseUrl,
      `/api/tasks/${encodeURIComponent(task.body.id)}/messages`,
      {
        method: "POST",
        headers: { authorization: `Bearer ${token}` },
        body: JSON.stringify({ text: "hello platform" }),
      },
    );
    assertStatus("send message", message.status, 200);

    const interrupt = await fetchJson(
      baseUrl,
      `/api/runs/${encodeURIComponent(run.body.run.id)}/interrupt`,
      {
        method: "POST",
        headers: { authorization: `Bearer ${token}` },
        body: JSON.stringify({ turn_id: "turn-smoke" }),
      },
    );
    assertStatus("interrupt run", interrupt.status, 200);

    const steer = await fetchJson(
      baseUrl,
      `/api/runs/${encodeURIComponent(run.body.run.id)}/steer`,
      {
        method: "POST",
        headers: { authorization: `Bearer ${token}` },
        body: JSON.stringify({ turn_id: "turn-smoke", text: "focus on tests" }),
      },
    );
    assertStatus("steer run", steer.status, 200);

    const events = await fetchJson(
      baseUrl,
      `/api/tasks/${encodeURIComponent(task.body.id)}/events?limit=10`,
      { headers: { authorization: `Bearer ${token}` } },
    );
    assertStatus("list task events", events.status, 200);
    if (!Array.isArray(events.body)) {
      throw new Error("task events response is not an array");
    }

    console.log(
      JSON.stringify(
        {
          ok: true,
          checks: [
            "legacy_proxy_removed",
            "delete_project",
            "active_run",
            "run_files",
            "run_file_content",
            "git_status",
            "send_message",
            "interrupt",
            "steer",
            "task_events",
          ],
          runId: run.body.run.id,
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

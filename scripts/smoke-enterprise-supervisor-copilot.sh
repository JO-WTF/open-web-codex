#!/usr/bin/env zsh
set -euo pipefail

script_dir="${0:A:h}"
repo_root="${script_dir:h}"
web_root="${repo_root}/apps/web"
codex_root="${repo_root}/codex/codex-rs"

if [[ "${1:-}" == "--help" ]]; then
  print "Usage: scripts/smoke-enterprise-supervisor-copilot.sh"
  print
  print "Runs the real single-Profile Enterprise Supervisor case against a"
  print "disposable PostgreSQL database, Profile and managed Workspace."
  print "Optional environment: CODEX_BIN, E2E_EVIDENCE_FILE, E2E_PROVIDER_ID,"
  print "E2E_MODEL, E2E_EFFORT, E2E_USE_BUILT_IN_PROVIDER,"
  print "E2E_PROMPT, E2E_CASE_NAME, E2E_OBSERVE_ONLY, E2E_LIFECYCLE_PROBE,"
  print "OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV,"
  print "OPEN_WEB_CODEX_E2E_PG_PORT and OPEN_WEB_CODEX_E2E_SERVER_PORT."
  exit 0
fi
if (( $# > 0 )); then
  print -u2 "Unknown argument: $1"
  exit 2
fi

codex_bin="${CODEX_BIN:-${codex_root}/target/debug/codex}"
server_bin="${web_root}/target/debug/open-web-codex-server"
planner_venv="${OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV:-${repo_root}/.local/open-web-codex/tool-envs/supply-chain-network-planner}"
e2e_root="$(mktemp -d /private/tmp/open-web-codex-enterprise-e2e.XXXXXX)"
pg_data="${e2e_root}/postgres"
pg_socket="${e2e_root}/socket"
profile_root="${e2e_root}/profile"
runner_root="${e2e_root}/runner"
data_root="${e2e_root}/data"
evidence_file="${E2E_EVIDENCE_FILE:-${e2e_root}/enterprise-supervisor-evidence.json}"
pg_port="${OPEN_WEB_CODEX_E2E_PG_PORT:-$(
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
)}"
server_port="${OPEN_WEB_CODEX_E2E_SERVER_PORT:-$(
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
)}"
server_pid=""

cleanup() {
  if [[ -n "${server_pid}" ]]; then
    kill "${server_pid}" >/dev/null 2>&1 || true
    wait "${server_pid}" >/dev/null 2>&1 || true
  fi
  /opt/homebrew/opt/postgresql@17/bin/pg_ctl \
    -D "${pg_data}" -m fast -w stop >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ ! -x "/opt/homebrew/opt/postgresql@17/bin/initdb" ]]; then
  print -u2 "PostgreSQL 17 is required at /opt/homebrew/opt/postgresql@17."
  exit 1
fi
if [[ ! -x "${planner_venv}/bin/python" ]]; then
  print -u2 "Supply-chain planner environment is missing: ${planner_venv}"
  exit 1
fi

if [[ ! -x "${codex_bin}" ]]; then
  (
    cd "${codex_root}"
    cargo build -p codex-cli
  )
fi
(
  cd "${web_root}"
  npm run build
  cargo build -p open-web-codex-server --locked
)

mkdir -p "${pg_socket}" "${profile_root}" "${runner_root}" "${data_root}/logs"
/opt/homebrew/opt/postgresql@17/bin/initdb \
  -D "${pg_data}" -A trust -U postgres --no-locale >/dev/null
/opt/homebrew/opt/postgresql@17/bin/pg_ctl \
  -D "${pg_data}" -o "-F -p ${pg_port} -k ${pg_socket}" -w start >/dev/null
/opt/homebrew/opt/postgresql@17/bin/createdb \
  -h "${pg_socket}" -p "${pg_port}" -U postgres enterprise_e2e

export DATABASE_URL="postgresql://postgres@localhost:${pg_port}/enterprise_e2e?host=${pg_socket}"
export CODEX_MODE="real"
export CODEX_HOME="${profile_root}"
export CODEX_BIN="${codex_bin}"
export OPEN_WEB_CODEX_RUNNER_ROOT="${runner_root}"
export OPEN_WEB_CODEX_DATA_DIR="${data_root}"
export OPEN_WEB_CODEX_LOG_DIR="${data_root}/logs"
export OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV="${planner_venv}"
export OPEN_WEB_CODEX_MASTER_KEY="$(openssl rand -base64 32)"
export OPEN_WEB_CODEX_WEB_DIST="${web_root}/dist"
export RUST_LOG="open_web_codex_server=info,open_web_codex_profile_host=info,warn"

"${server_bin}" --bind "127.0.0.1:${server_port}" \
  >"${e2e_root}/server.log" 2>&1 &
server_pid="$!"

for _ in {1..120}; do
  if curl --silent --fail "http://127.0.0.1:${server_port}/api/health" >/dev/null; then
    break
  fi
  if ! kill -0 "${server_pid}" >/dev/null 2>&1; then
    sed -n '1,260p' "${e2e_root}/server.log"
    exit 1
  fi
  sleep 1
done
curl --silent --fail "http://127.0.0.1:${server_port}/api/health" >/dev/null

(
  cd "${web_root}"
  E2E_BASE_URL="http://127.0.0.1:${server_port}" \
    E2E_EVIDENCE_FILE="${evidence_file}" \
    E2E_PROVIDER_ID="${E2E_PROVIDER_ID:-openai}" \
    E2E_MODEL="${E2E_MODEL:-gpt-5.6-sol}" \
    E2E_EFFORT="${E2E_EFFORT:-medium}" \
    E2E_USE_BUILT_IN_PROVIDER="${E2E_USE_BUILT_IN_PROVIDER:-1}" \
    npm run test:e2e:enterprise-supervisor
)

node - "${profile_root}" "${evidence_file}" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");

const [, , profileRoot, evidenceFile] = process.argv;
const evidence = JSON.parse(fs.readFileSync(evidenceFile, "utf8"));
const expectedThreadIds = new Set(
  evidence.agents.map((agent) => agent.thread_id).filter(Boolean),
);
const observedVersions = new Map();

function visit(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      visit(entryPath);
      continue;
    }
    if (!entry.name.endsWith(".jsonl")) {
      continue;
    }
    const firstLine = fs.readFileSync(entryPath, "utf8").split("\n", 1)[0];
    const event = JSON.parse(firstLine);
    if (event.type !== "session_meta") {
      continue;
    }
    const threadId = event.payload?.id;
    if (expectedThreadIds.has(threadId)) {
      observedVersions.set(threadId, event.payload?.multi_agent_version);
    }
  }
}

visit(path.join(profileRoot, "sessions"));
const failures = [...expectedThreadIds].filter(
  (threadId) => observedVersions.get(threadId) !== "v2",
);
if (failures.length > 0) {
  throw new Error(
    `Governed Agent threads did not all use Multi-Agent V2: ${failures
      .map((threadId) => `${threadId}=${observedVersions.get(threadId) ?? "missing"}`)
      .join(", ")}`,
  );
}
console.log(`Verified Multi-Agent V2 for ${expectedThreadIds.size} governed threads.`);
NODE

print "E2E_ROOT=${e2e_root}"
print "EVIDENCE_FILE=${evidence_file}"

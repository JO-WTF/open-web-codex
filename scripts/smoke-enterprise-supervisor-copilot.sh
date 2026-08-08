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
capability_root_dir="${e2e_root}/capability-roots"
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

mkdir -p "${pg_socket}" "${profile_root}" "${runner_root}" "${data_root}/logs" "${capability_root_dir}"
cp -R "${repo_root}/tools/maps-mcp" "${capability_root_dir}/maps-mcp"
cp -R "${repo_root}/tools/supply-chain-network-planner" "${capability_root_dir}/supply-chain-network-planner"
rm -rf "${capability_root_dir}/maps-mcp/node_modules" \
  "${capability_root_dir}/maps-mcp/.venv" \
  "${capability_root_dir}/supply-chain-network-planner/.venv"
/opt/homebrew/opt/postgresql@17/bin/initdb \
  -D "${pg_data}" -A trust -U postgres --no-locale --encoding=UTF8 >/dev/null
/opt/homebrew/opt/postgresql@17/bin/pg_ctl \
  -D "${pg_data}" -o "-F -p ${pg_port} -k ${pg_socket}" -w start >/dev/null
/opt/homebrew/opt/postgresql@17/bin/createdb \
  -h "${pg_socket}" -p "${pg_port}" -U postgres enterprise_e2e
[[ "$(
  /opt/homebrew/opt/postgresql@17/bin/psql \
    -h "${pg_socket}" -p "${pg_port}" -U postgres -d enterprise_e2e \
    -Atc "SHOW server_encoding"
)" == "UTF8" ]]

export DATABASE_URL="postgresql://postgres@localhost:${pg_port}/enterprise_e2e?host=${pg_socket}"
export CODEX_MODE="real"
export CODEX_HOME="${profile_root}"
export CODEX_BIN="${codex_bin}"
export OPEN_WEB_CODEX_RUNNER_ROOT="${runner_root}"
export OPEN_WEB_CODEX_DATA_DIR="${data_root}"
export OPEN_WEB_CODEX_LOG_DIR="${data_root}/logs"
export OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV="${planner_venv}"
export OPEN_WEB_CODEX_CAPABILITY_ROOTS="${capability_root_dir}/maps-mcp:${capability_root_dir}/supply-chain-network-planner"
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
    E2E_MODEL="${E2E_MODEL:-gpt-5.1-codex}" \
    E2E_EFFORT="${E2E_EFFORT:-medium}" \
    E2E_USE_BUILT_IN_PROVIDER="${E2E_USE_BUILT_IN_PROVIDER:-1}" \
    npm run test:e2e:enterprise-supervisor
)

node "${web_root}/scripts/verify-enterprise-agent-modes.mjs" \
  "${profile_root}" "${evidence_file}"

print "E2E_ROOT=${e2e_root}"
print "EVIDENCE_FILE=${evidence_file}"

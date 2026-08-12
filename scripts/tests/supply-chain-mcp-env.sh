#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-supply-chain-env.XXXXXX")"
setup_venv="$fixture_root/setup-venv"
launcher_venv="$fixture_root/launcher-venv"
asset_root="$fixture_root/supply-chain-network-planner"
install_marker="$fixture_root/setup-dependencies-installed"
install_log="$fixture_root/pip-installs"
server_exec_log="$fixture_root/server-execs"
setup_log="$fixture_root/setup.log"

cleanup() {
  rm -rf "$fixture_root"
}
trap cleanup EXIT

mkdir -p "$setup_venv/bin" "$launcher_venv/bin" "$asset_root"
cat >"$asset_root/pyproject.toml" <<'EOF'
[project]
name = "open-web-codex-supply-chain-network-planner"
version = "0.0.0"
dependencies = ["declared-solver"]
EOF

make_fake_python() {
  local destination="$1"
  cp "$fixture_root/fake-python" "$destination"
  chmod +x "$destination"
}

cat >"$fixture_root/fake-python" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

case "${1-}" in
  --version)
    printf 'Python 3.12.0\n'
    ;;
  -)
    probe="$(</dev/stdin)"
    [[ "$probe" == *"open-web-codex-supply-chain-network-planner"* ]]
    printf 'supply-chain MCP imports ok\n'
    ;;
  -m)
    case "${2-}" in
      pip)
        if [[ "${3-}" == "--version" ]]; then
          printf 'pip 25.0\n'
        elif [[ "${3-}" == "check" ]]; then
          if [[ ! -e "${SUPPLY_CHAIN_TEST_INSTALL_MARKER:?}" ]]; then
            printf 'declared-solver is not installed\n' >&2
            exit 1
          fi
          printf 'No broken requirements found.\n'
        elif [[ "${3-}" == "install" ]]; then
          touch "${SUPPLY_CHAIN_TEST_INSTALL_MARKER:?}"
          printf 'install\n' >>"${SUPPLY_CHAIN_TEST_INSTALL_LOG:?}"
        else
          printf 'unexpected pip arguments: %s\n' "$*" >&2
          exit 2
        fi
        ;;
      supply_chain_planner.server|supply_chain_planner.data_server)
        printf '%s\n' "${2-}" >>"${SUPPLY_CHAIN_TEST_SERVER_EXEC_LOG:?}"
        ;;
      *)
        printf 'unexpected module: %s\n' "${2-}" >&2
        exit 2
        ;;
    esac
    ;;
  *)
    printf 'unexpected fake Python arguments: %s\n' "$*" >&2
    exit 2
    ;;
esac
EOF

make_fake_python "$setup_venv/bin/python"
make_fake_python "$launcher_venv/bin/python"

run_setup() {
  SUPPLY_CHAIN_TEST_INSTALL_MARKER="$install_marker" \
    SUPPLY_CHAIN_TEST_INSTALL_LOG="$install_log" \
    SUPPLY_CHAIN_TEST_SERVER_EXEC_LOG="$server_exec_log" \
    PYTHON="$setup_venv/bin/python" \
    OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV="$setup_venv" \
    OPEN_WEB_CODEX_SUPPLY_CHAIN_ASSET_ROOT="$asset_root" \
    OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_SETUP_LOG="$setup_log" \
    "$repo_root/scripts/setup-supply-chain-mcp-env.sh" >/dev/null
}

run_setup
[[ -e "$install_marker" ]]
[[ "$(wc -l <"$install_log" | tr -d '[:space:]')" == "1" ]]
grep -F "installing or refreshing supply-chain MCP dependencies" "$setup_log" >/dev/null

run_setup
[[ "$(wc -l <"$install_log" | tr -d '[:space:]')" == "1" ]]
grep -F "supply-chain MCP dependencies already satisfy the current manifest" "$setup_log" >/dev/null

rm "$install_marker"
run_setup
[[ "$(wc -l <"$install_log" | tr -d '[:space:]')" == "2" ]]
grep -F "installed environment does not satisfy declared dependencies" "$setup_log" >/dev/null

printf '\n# dependency change\n' >>"$asset_root/pyproject.toml"
run_setup
[[ "$(wc -l <"$install_log" | tr -d '[:space:]')" == "3" ]]
grep -F "dependency manifest changed or was not previously recorded" "$setup_log" >/dev/null

launcher_marker="$fixture_root/launcher-dependencies-installed"
launcher_stderr="$fixture_root/launcher.stderr"
set +e
SUPPLY_CHAIN_TEST_INSTALL_MARKER="$launcher_marker" \
  SUPPLY_CHAIN_TEST_INSTALL_LOG="$install_log" \
  SUPPLY_CHAIN_TEST_SERVER_EXEC_LOG="$server_exec_log" \
  OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV="$launcher_venv" \
  OPEN_WEB_CODEX_DATA_DIR="$fixture_root/data" \
  "$repo_root/tools/supply-chain-network-planner/bin/supply-chain-planner-launcher" \
  >"$fixture_root/launcher.stdout" 2>"$launcher_stderr"
launcher_status=$?
set -e

[[ "$launcher_status" == "70" ]]
grep -F "does not satisfy dependencies declared in pyproject.toml" "$launcher_stderr" >/dev/null
[[ ! -e "$server_exec_log" ]]

touch "$launcher_marker"
SUPPLY_CHAIN_TEST_INSTALL_MARKER="$launcher_marker" \
  SUPPLY_CHAIN_TEST_INSTALL_LOG="$install_log" \
  SUPPLY_CHAIN_TEST_SERVER_EXEC_LOG="$server_exec_log" \
  OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV="$launcher_venv" \
  OPEN_WEB_CODEX_DATA_DIR="$fixture_root/data" \
  "$repo_root/tools/supply-chain-network-planner/bin/supply-chain-planner-launcher"
grep -Fx "supply_chain_planner.server" "$server_exec_log" >/dev/null

printf 'supply-chain MCP environment tests passed\n'

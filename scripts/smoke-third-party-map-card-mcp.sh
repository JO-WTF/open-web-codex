#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
codex_bin="${CODEX_BIN:-$repo_root/codex/codex-rs/target/debug/codex}"
provider_id="${THIRD_PARTY_PROVIDER_ID:-deepseek}"
provider_name="${THIRD_PARTY_PROVIDER_NAME:-DeepSeek}"
provider_base_url="${THIRD_PARTY_PROVIDER_BASE_URL:-https://api.deepseek.com/v1}"
provider_env_key="${THIRD_PARTY_PROVIDER_ENV_KEY:-DEEPSEEK_API_KEY}"
model="${THIRD_PARTY_PROVIDER_MODEL:-deepseek-chat}"
timeout_sec="${THIRD_PARTY_SMOKE_TIMEOUT_SEC:-180}"
data_dir="${OPEN_WEB_CODEX_DATA_DIR:-$repo_root/.local/open-web-codex}"
maps_mcp_venv="${OPEN_WEB_CODEX_MAPS_MCP_VENV:-${MAPS_MCP_VENV:-$data_dir/tool-envs/maps-mcp}}"

if [[ ! -x "$codex_bin" ]]; then
  echo "Codex binary not found. Build it first or set CODEX_BIN: $codex_bin" >&2
  exit 1
fi
if [[ -z "${!provider_env_key:-}" ]]; then
  echo "Missing provider key environment variable: $provider_env_key" >&2
  exit 2
fi

tmp_home="$(mktemp -d)"
trap 'rm -rf "$tmp_home"' EXIT
OPEN_WEB_CODEX_MAPS_MCP_VENV="$maps_mcp_venv" "$repo_root/scripts/setup-maps-mcp-env.sh" >/dev/null
cat > "$tmp_home/config.toml" <<TOML
model = "$model"
model_provider = "$provider_id"
approval_policy = "never"
sandbox_mode = "read-only"

[model_providers.$provider_id]
name = "$provider_name"
base_url = "$provider_base_url"
env_key = "$provider_env_key"
wire_api = "chat"
request_max_retries = 0
stream_max_retries = 0
models = [{ model_id = "$model", model_name = "$model", context_window = 64000, show_in_picker = true }]

[mcp_servers.map_utils]
command = "$repo_root/tools/maps-mcp/bin/maps-mcp-launcher"
args = ["--workspace-root", "$repo_root"]
cwd = "$repo_root/tools/maps-mcp"
startup_timeout_sec = 120
tool_timeout_sec = 180

[mcp_servers.map_utils.tools.create_map_card]
approval_mode = "approve"
TOML

output_file="${THIRD_PARTY_SMOKE_OUTPUT:-$tmp_home/third-party-map-card-smoke.jsonl}"
OPEN_WEB_CODEX_MAPS_MCP_VENV="$maps_mcp_venv" MAPS_MCP_VENV="$maps_mcp_venv" CODEX_HOME="$tmp_home" timeout "$timeout_sec" "$codex_bin" exec --json --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox --ignore-rules -C "$repo_root" \
  '用地图卡片展示雅加达坐标 106.827168,-6.1754049。必须调用 map_utils 的 create_map_card，使用 inline GeoJSON source、point layer 和 camera zoom 10，并简要说明已经生成地图，不要复制 JSON。' \
  | tee "$output_file"

if ! jq -e '
  select(
    .type == "item.completed"
    and .item.type == "mcp_tool_call"
    and .item.server == "map_utils"
    and .item.tool == "create_map_card"
  )
  | .item.result.structured_content
  | .type == "open-web-card"
    and .kind == "map.v2"
    and (.card | type == "object")
    and (.card.title | type == "string")
    and (.card.viewport.mode == "camera")
    and (.card.viewport.zoom == 10)
    and (.card.sources | length == 1)
    and (.card.layers[0].geometry == "point")
' "$output_file" >/dev/null; then
  echo "third-party map-card MCP smoke did not observe valid structuredContent" >&2
  exit 3
fi

echo "third-party map-card MCP smoke passed: $output_file"

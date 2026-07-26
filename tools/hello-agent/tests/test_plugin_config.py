from __future__ import annotations

import json
from pathlib import Path


def test_plugin_manifest_and_two_mcp_servers_are_wired() -> None:
    root = Path(__file__).resolve().parents[1]
    manifest = json.loads((root / ".codex-plugin" / "plugin.json").read_text())
    mcp_config = json.loads((root / ".mcp.json").read_text())

    assert manifest["name"] == "hello-agent"
    assert manifest["skills"] == "./skills/"
    assert manifest["mcpServers"] == "./.mcp.json"

    writer = mcp_config["mcpServers"]["hello_writer"]
    reviewer = mcp_config["mcpServers"]["hello_reviewer"]
    assert writer["command"] == "./bin/hello-agent-launcher"
    assert writer["args"] == []
    assert reviewer["command"] == "./bin/hello-agent-launcher"
    assert reviewer["args"] == ["--reviewer-server"]

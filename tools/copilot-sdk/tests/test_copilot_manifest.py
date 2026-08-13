from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from copilot_sdk.copilot_manifest import CopilotPackageError, validate_copilot_package


class CopilotManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary_directory.name)
        (self.root / "skills" / "supervisor").mkdir(parents=True)
        (self.root / "skills" / "worker").mkdir(parents=True)
        (self.root / "agents").mkdir()
        (self.root / "tools" / "routes" / ".codex-plugin").mkdir(parents=True)
        (self.root / "skills" / "supervisor" / "SKILL.md").write_text(
            "---\nname: supervisor\ndescription: Coordinate work.\n---\n# Supervisor\n",
            encoding="utf-8",
        )
        (self.root / "skills" / "worker" / "SKILL.md").write_text(
            "---\nname: worker\ndescription: Perform work.\n---\n# Worker\n",
            encoding="utf-8",
        )
        (self.root / "agents" / "planner.toml").write_text(
            "name = \"planner\"\n\n"
            "[skills]\nconfig = [{ name = \"worker\", enabled = true }]\n\n"
            "[plugins.routes]\nenabled = true\n"
            "[plugins.routes.mcp_servers.routes]\n"
            "enabled_tools = [\"health\"]\n",
            encoding="utf-8",
        )
        (self.root / "tools" / "routes" / ".codex-plugin" / "plugin.json").write_text(
            '{"name":"routes","version":"1.0.0","mcpServers":"./.mcp.json"}\n',
            encoding="utf-8",
        )
        (self.root / "tools" / "routes" / ".mcp.json").write_text(
            '{"mcpServers":{"routes":{"command":"ignored"}}}\n',
            encoding="utf-8",
        )
        self.write_manifest()

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def write_manifest(self, replacement: str | None = None) -> None:
        content = replacement or """schema_version = 1
id = "network-copilot"
display_name = "Network Copilot"

[supervisor]
skill = "supervisor"

[[skills]]
id = "supervisor"
path = "skills/supervisor"

[[skills]]
id = "worker"
path = "skills/worker"

[[agents]]
id = "planner"
role = "agents/planner.toml"

[[tools]]
id = "routes"
root = "tools/routes"
"""
        (self.root / "copilot.toml").write_text(content, encoding="utf-8")

    def assert_code(self, expected: str) -> None:
        with self.assertRaises(CopilotPackageError) as caught:
            validate_copilot_package(self.root)
        self.assertEqual(caught.exception.code, expected)

    def test_valid_package_returns_stable_summary(self) -> None:
        first = validate_copilot_package(self.root)
        second = validate_copilot_package(self.root, Path("copilot.toml"))
        self.assertEqual(first, second)
        self.assertEqual(first.id, "network-copilot")
        self.assertEqual(first.skill_ids, ("supervisor", "worker"))
        self.assertEqual(first.agent_ids, ("planner",))
        self.assertEqual(first.tool_ids, ("routes",))
        self.assertRegex(first.composition_descriptor_sha256, r"^[0-9a-f]{64}$")

    def test_rejects_absolute_authored_path(self) -> None:
        manifest = (self.root / "copilot.toml").read_text(encoding="utf-8")
        self.write_manifest(manifest.replace('path = "skills/worker"', 'path = "/tmp/worker"'))
        self.assert_code("invalid_path")

    def test_rejects_absolute_manifest_inside_source_root(self) -> None:
        with self.assertRaises(CopilotPackageError) as caught:
            validate_copilot_package(self.root, self.root / "copilot.toml")
        self.assertEqual(caught.exception.code, "invalid_path")

    def test_rejects_parent_traversal(self) -> None:
        manifest = (self.root / "copilot.toml").read_text(encoding="utf-8")
        self.write_manifest(manifest.replace('role = "agents/planner.toml"', 'role = "../planner.toml"'))
        self.assert_code("invalid_path")

    def test_rejects_manifest_escape(self) -> None:
        with self.assertRaises(CopilotPackageError) as caught:
            validate_copilot_package(self.root, Path("../copilot.toml"))
        self.assertEqual(caught.exception.code, "invalid_path")

    def test_rejects_symlink_component(self) -> None:
        actual = self.root / "actual"
        actual.mkdir()
        (actual / "SKILL.md").write_text("---\nname: worker\n---\n", encoding="utf-8")
        skill = self.root / "skills" / "worker"
        skill.rename(self.root / "unused-worker")
        skill.symlink_to(actual, target_is_directory=True)
        self.assert_code("unsafe_symlink")

    def test_rejects_duplicate_ids(self) -> None:
        manifest = (self.root / "copilot.toml").read_text(encoding="utf-8")
        self.write_manifest(manifest.replace('id = "worker"', 'id = "supervisor"'))
        self.assert_code("duplicate_id")

    def test_requires_supervisor(self) -> None:
        manifest = (self.root / "copilot.toml").read_text(encoding="utf-8")
        self.write_manifest(manifest.replace('[supervisor]\nskill = "supervisor"\n\n', ""))
        self.assert_code("required_field")

    def test_rejects_skill_frontmatter_mismatch(self) -> None:
        (self.root / "skills" / "worker" / "SKILL.md").write_text(
            "---\nname: another-worker\ndescription: Perform work.\n---\n", encoding="utf-8"
        )
        self.assert_code("frontmatter_mismatch")

    def test_rejects_invalid_skill_description(self) -> None:
        (self.root / "skills" / "worker" / "SKILL.md").write_text(
            "---\nname: worker\ndescription: |\n  multiline\n---\n", encoding="utf-8"
        )
        self.assert_code("frontmatter_mismatch")

    def test_rejects_invalid_skill_yaml_subset(self) -> None:
        (self.root / "skills" / "worker" / "SKILL.md").write_text(
            "---\nname: worker\ndescription: Perform work.\nunknown: value\n---\n",
            encoding="utf-8",
        )
        self.assert_code("frontmatter_mismatch")

    def test_rejects_invalid_skill_name_rule(self) -> None:
        (self.root / "skills" / "worker" / "SKILL.md").write_text(
            "---\nname: Worker_Name\ndescription: Perform work.\n---\n", encoding="utf-8"
        )
        self.assert_code("frontmatter_mismatch")

    def test_rejects_agent_role_name_mismatch(self) -> None:
        role = self.root / "agents" / "planner.toml"
        role.write_text(role.read_text(encoding="utf-8").replace('name = "planner"', 'name = "other"'), encoding="utf-8")
        self.assert_code("missing_reference")

    def test_rejects_dangling_role_skill(self) -> None:
        role = self.root / "agents" / "planner.toml"
        role.write_text(role.read_text(encoding="utf-8").replace("worker", "ghost"), encoding="utf-8")
        self.assert_code("missing_reference")

    def test_rejects_dangling_role_tool(self) -> None:
        role = self.root / "agents" / "planner.toml"
        role.write_text(role.read_text(encoding="utf-8").replace("routes", "ghost"), encoding="utf-8")
        self.assert_code("missing_reference")

    def test_rejects_authored_role_transport(self) -> None:
        role = self.root / "agents" / "planner.toml"
        role.write_text(
            'name = "planner"\n[mcp_servers.routes]\ncommand = "./launcher"\n',
            encoding="utf-8",
        )
        self.assert_code("invalid_field")

    def test_rejects_missing_plugin_descriptor(self) -> None:
        (self.root / "tools" / "routes" / ".codex-plugin" / "plugin.json").unlink()
        self.assert_code("missing_file")

    def test_rejects_missing_mcp_descriptor(self) -> None:
        (self.root / "tools" / "routes" / ".mcp.json").unlink()
        self.assert_code("missing_file")

    def test_rejects_plugin_mcp_reference_mismatch(self) -> None:
        plugin = self.root / "tools" / "routes" / ".codex-plugin" / "plugin.json"
        plugin.write_text('{"mcpServers":"../.mcp.json"}', encoding="utf-8")
        self.assert_code("invalid_type")

    def test_rejects_missing_declared_mcp_server(self) -> None:
        mcp = self.root / "tools" / "routes" / ".mcp.json"
        mcp.write_text('{"mcpServers":{"other":{}}}', encoding="utf-8")
        self.assert_code("missing_reference")


if __name__ == "__main__":
    unittest.main()

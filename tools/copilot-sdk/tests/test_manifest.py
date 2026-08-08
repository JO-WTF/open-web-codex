import tempfile
import unittest
from pathlib import Path

from copilot_sdk.manifest import PackageError, pack_tool_package, validate_tool_package


class ManifestTests(unittest.TestCase):
    def test_valid_package_has_stable_hash(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "tool"
            (root / ".codex-plugin").mkdir(parents=True)
            (root / ".codex-plugin/plugin.json").write_text(
                '{"name":"example","version":"0.1.0","mcpServers":"./.mcp.json"}',
                encoding="utf-8",
            )
            (root / ".mcp.json").write_text(
                '{"mcpServers":{"example":{"command":"./bin/example"}}}',
                encoding="utf-8",
            )
            first = validate_tool_package(root)
            second = validate_tool_package(root)
            self.assertEqual(first["contentSha256"], second["contentSha256"])
            summary = pack_tool_package(root, Path(directory) / "example.zip")
            self.assertEqual(summary["contentSha256"], first["contentSha256"])

    def test_missing_mcp_server_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / ".codex-plugin").mkdir()
            (root / ".codex-plugin/plugin.json").write_text(
                '{"name":"example","version":"0.1.0","mcpServers":"./.mcp.json"}',
                encoding="utf-8",
            )
            (root / ".mcp.json").write_text('{"mcpServers":{}}', encoding="utf-8")
            with self.assertRaises(PackageError):
                validate_tool_package(root)


if __name__ == "__main__":
    unittest.main()

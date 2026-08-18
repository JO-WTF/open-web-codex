from __future__ import annotations

import tomllib
import unittest
from pathlib import Path


PACKAGE_ROOT = Path(__file__).resolve().parents[1]
REPOSITORY_ROOT = PACKAGE_ROOT.parents[1]
TOOL_ROOT = REPOSITORY_ROOT / "tools"


class WarehouseNetworkLayoutTests(unittest.TestCase):
    def test_copilot_owns_prompts_and_uses_shared_root_tools(self) -> None:
        self.assertTrue((PACKAGE_ROOT / "copilot.toml").is_file())
        self.assertTrue((PACKAGE_ROOT / "agents").is_dir())
        self.assertTrue((PACKAGE_ROOT / "skills").is_dir())
        self.assertTrue((TOOL_ROOT / "warehouse-network-planner/tool.toml").is_file())
        self.assertTrue((TOOL_ROOT / "warehouse-network-maps/tool.toml").is_file())
        self.assertFalse((PACKAGE_ROOT / "tools").exists())

    def test_planner_has_one_physical_source_root(self) -> None:
        planner = TOOL_ROOT / "warehouse-network-planner"
        self.assertFalse((planner / "supply_chain_planner").exists())
        self.assertFalse((planner / "planner").exists())
        self.assertEqual(
            {
                path.name
                for path in (planner / "src").iterdir()
                if path.is_dir() and path.name != "__pycache__"
            },
            {"data", "delivery", "network", "shared"},
        )

        project = tomllib.loads((planner / "pyproject.toml").read_text())
        setuptools = project["tool"]["setuptools"]
        self.assertEqual(setuptools["package-dir"], {"supply_chain_planner": "src"})
        self.assertIn("supply_chain_planner.network", setuptools["packages"])
        self.assertIn("supply_chain_planner.data", setuptools["packages"])

    def test_manifest_paths_are_package_relative(self) -> None:
        manifest = tomllib.loads((PACKAGE_ROOT / "copilot.toml").read_text())
        for entry in manifest["skills"]:
            self.assertNotIn("..", Path(entry["path"]).parts)
        for entry in manifest["agents"]:
            self.assertNotIn("..", Path(entry["role"]).parts)
        for entry in manifest["tools"]:
            self.assertEqual(set(entry), {"id", "package"})


if __name__ == "__main__":
    unittest.main()

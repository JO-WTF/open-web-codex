from __future__ import annotations

import tomllib
import unittest
from pathlib import Path


PACKAGE_ROOT = Path(__file__).resolve().parents[1]


class WarehouseNetworkLayoutTests(unittest.TestCase):
    def test_developer_sources_share_one_copilot_root(self) -> None:
        self.assertTrue((PACKAGE_ROOT / "copilot.toml").is_file())
        self.assertTrue((PACKAGE_ROOT / "agents").is_dir())
        self.assertTrue((PACKAGE_ROOT / "skills").is_dir())
        self.assertTrue((PACKAGE_ROOT / "tools/planner").is_dir())
        self.assertTrue((PACKAGE_ROOT / "tools/maps").is_dir())

    def test_planner_has_one_physical_source_root(self) -> None:
        planner = PACKAGE_ROOT / "tools/planner"
        self.assertFalse((planner / "supply_chain_planner").exists())
        self.assertFalse((planner / "planner").exists())
        self.assertEqual(
            {
                path.name
                for path in (planner / "src").iterdir()
                if path.is_dir() and path.name != "__pycache__"
            },
            {"data", "delivery", "network", "resources", "shared"},
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
            self.assertNotIn("..", Path(entry["root"]).parts)
            self.assertNotIn("..", Path(entry["runtime"]).parts)


if __name__ == "__main__":
    unittest.main()

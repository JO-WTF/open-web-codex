from __future__ import annotations

import json
import stat
import tempfile
import unittest
from pathlib import Path

from maps_mcp.credentials import WorkspaceCredentialStore


class WorkspaceCredentialStoreTests(unittest.TestCase):
    def test_round_trip_is_workspace_scoped_and_owner_only(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            store = WorkspaceCredentialStore(Path(temp_dir))

            self.assertIsNone(store.get_api_key("google"))
            store.set_api_key("google", "google-secret")
            store.set_api_key("mapbox", "mapbox-secret")

            self.assertEqual(store.get_api_key("google"), "google-secret")
            self.assertEqual(store.get_api_key("mapbox"), "mapbox-secret")
            self.assertEqual(stat.S_IMODE(store.memory_path.stat().st_mode), 0o600)
            data = json.loads(store.memory_path.read_text())
            self.assertEqual(data["version"], 1)

    def test_rejects_unknown_provider(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            store = WorkspaceCredentialStore(Path(temp_dir))
            with self.assertRaisesRegex(ValueError, "Unsupported maps provider"):
                store.set_api_key("other", "secret")


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

import io
import os
import signal
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from copilot_sdk.app_server_client import AppServerClient, AppServerClientError


class _FakeProcess:
    def __init__(self) -> None:
        self.pid = 424242
        self.stdin = io.StringIO()
        self.stdout = io.StringIO()
        self.stderr = io.StringIO()
        self.returncode: int | None = None
        self.signals: list[int] = []

    def poll(self) -> int | None:
        return self.returncode

    def wait(self, timeout: float | None = None) -> int:
        self.returncode = 0
        return 0

    def send_signal(self, value: int) -> None:
        self.signals.append(value)

    def kill(self) -> None:
        self.returncode = -9


class AppServerLifecycleTests(unittest.TestCase):
    def test_launch_and_close_own_a_process_group_without_targeting_caller(self) -> None:
        fake = _FakeProcess()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch("copilot_sdk.app_server_client.subprocess.Popen", return_value=fake) as popen:
                client = AppServerClient.launch(
                    root / "codex",
                    profile_root=root / "profile",
                    process_home=root / "home",
                    process_cwd=root,
                )
            kwargs = popen.call_args.kwargs
            if os.name == "nt":
                self.assertTrue(
                    kwargs["creationflags"] & subprocess.CREATE_NEW_PROCESS_GROUP
                )
                client.close()
                self.assertEqual(fake.signals, [signal.CTRL_BREAK_EVENT])
            else:
                self.assertTrue(kwargs["start_new_session"])
                with patch("copilot_sdk.app_server_client.os.killpg") as killpg:
                    client.close()
                killpg.assert_called_once_with(fake.pid, signal.SIGTERM)
                self.assertNotEqual(fake.pid, os.getpgrp())


if __name__ == "__main__":
    unittest.main()

"""Minimal synchronous JSONL client for a one-shot Codex app-server probe."""

from __future__ import annotations

import json
import os
import queue
import subprocess
import threading
from contextlib import suppress
from pathlib import Path
from typing import Any


class AppServerClientError(RuntimeError):
    def __init__(self, code: str, message: str, cause: str | None = None) -> None:
        self.code = code
        self.message = message
        self.cause = cause
        super().__init__(f"{code}: {message}")


class AppServerClient:
    """Own an app-server subprocess and correlate newline-delimited responses."""

    def __init__(
        self,
        process: subprocess.Popen[str],
        *,
        timeout_seconds: float = 15.0,
    ) -> None:
        self.process = process
        self.timeout_seconds = timeout_seconds
        self._next_id = 1
        self._messages: queue.Queue[dict[str, Any] | BaseException | None] = queue.Queue()
        self._stderr: list[str] = []
        self._stdout_thread = threading.Thread(
            target=self._read_stdout,
            name="copilot-app-server-stdout",
            daemon=True,
        )
        self._stderr_thread = threading.Thread(
            target=self._read_stderr,
            name="copilot-app-server-stderr",
            daemon=True,
        )
        self._stdout_thread.start()
        self._stderr_thread.start()

    @classmethod
    def launch(
        cls,
        codex_bin: Path,
        *,
        profile_root: Path,
        process_home: Path,
        process_cwd: Path,
        environment: dict[str, str] | None = None,
        timeout_seconds: float = 15.0,
    ) -> AppServerClient:
        process_environment = os.environ.copy()
        process_environment.update(
            {
                "CODEX_HOME": str(profile_root),
                "HOME": str(process_home),
                "USERPROFILE": str(process_home),
            }
        )
        if environment is not None:
            process_environment.update(environment)
        try:
            process = subprocess.Popen(
                [str(codex_bin), "app-server"],
                cwd=process_cwd,
                env=process_environment,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                encoding="utf-8",
                bufsize=1,
            )
        except OSError as exc:
            raise AppServerClientError("AppServerUnavailable", "could not start app-server", str(exc)) from exc
        return cls(process, timeout_seconds=timeout_seconds)

    def request(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        request_id = self._next_id
        self._next_id += 1
        self._write({"id": request_id, "method": method, "params": params})
        while True:
            try:
                message = self._messages.get(timeout=self.timeout_seconds)
            except queue.Empty as exc:
                raise AppServerClientError("RpcFailed", f"timed out waiting for {method}", self.stderr) from exc
            if message is None:
                raise AppServerClientError("RpcFailed", f"app-server closed during {method}", self.stderr)
            if isinstance(message, BaseException):
                raise AppServerClientError("RpcFailed", f"invalid app-server output during {method}", str(message))
            if message.get("id") != request_id:
                continue
            if "error" in message:
                raise AppServerClientError("RpcFailed", f"{method} was rejected", json.dumps(message["error"], ensure_ascii=False))
            result = message.get("result")
            if not isinstance(result, dict):
                raise AppServerClientError("RpcFailed", f"{method} returned a non-object result")
            return result

    def notify(self, method: str, params: dict[str, Any] | None = None) -> None:
        payload: dict[str, Any] = {"method": method}
        if params is not None:
            payload["params"] = params
        self._write(payload)

    def initialize(self) -> dict[str, Any]:
        result = self.request(
            "initialize",
            {
                "clientInfo": {
                    "name": "open-web-codex-copilot-sdk",
                    "title": "Copilot SDK",
                    "version": "0.1.0",
                },
                "capabilities": {"experimentalApi": True},
            },
        )
        self.notify("initialized")
        return result

    @property
    def stderr(self) -> str:
        return "".join(self._stderr)[-8000:]

    def close(self) -> None:
        if self.process.stdin is not None and not self.process.stdin.closed:
            with suppress(OSError):
                self.process.stdin.close()
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)
        self._stdout_thread.join(timeout=1)
        self._stderr_thread.join(timeout=1)
        for stream in (self.process.stdout, self.process.stderr):
            if stream is not None and not stream.closed:
                with suppress(OSError):
                    stream.close()

    def _write(self, payload: dict[str, Any]) -> None:
        if self.process.stdin is None:
            raise AppServerClientError("RpcFailed", "app-server stdin is unavailable")
        try:
            self.process.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n")
            self.process.stdin.flush()
        except (BrokenPipeError, OSError) as exc:
            raise AppServerClientError("RpcFailed", "could not write to app-server", str(exc)) from exc

    def _read_stdout(self) -> None:
        assert self.process.stdout is not None
        try:
            for line in self.process.stdout:
                value = json.loads(line)
                if not isinstance(value, dict):
                    raise ValueError("JSONL message must be an object")
                self._messages.put(value)
        except BaseException as exc:
            self._messages.put(exc)
        finally:
            self._messages.put(None)

    def _read_stderr(self) -> None:
        assert self.process.stderr is not None
        for line in self.process.stderr:
            self._stderr.append(line)

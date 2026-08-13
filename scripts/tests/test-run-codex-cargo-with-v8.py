#!/usr/bin/env python3

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch


REPO_ROOT = Path(__file__).resolve().parents[2]
ADAPTER_PATH = REPO_ROOT / "scripts" / "run-codex-cargo-with-v8.py"
MODULE_SPEC = importlib.util.spec_from_file_location("codex_cargo_adapter", ADAPTER_PATH)
assert MODULE_SPEC is not None and MODULE_SPEC.loader is not None
adapter = importlib.util.module_from_spec(MODULE_SPEC)
MODULE_SPEC.loader.exec_module(adapter)


class CodexCargoAdapterTests(unittest.TestCase):
    def test_native_target_and_v8_environment_are_forwarded_to_cargo(self) -> None:
        captured: dict[str, object] = {}

        def fake_execvpe(command, argv, environment):
            captured.update(command=command, argv=argv, environment=environment)

        target = "aarch64-apple-darwin"
        resolver_input: dict[str, str] = {}

        def fake_resolver(spec, *, environ):
            resolver_input.update(environ)
            return {
                "RUSTY_V8_ARCHIVE": "/tmp/v8/archive.a.gz",
                "RUSTY_V8_SRC_BINDING_PATH": "/tmp/v8/binding.rs",
            }

        with (
            patch.object(adapter, "default_target", return_value=target),
            patch.object(adapter, "configure_native_certificate_store") as configure_trust,
            patch.object(
                adapter,
                "resolve_codex_v8_cargo_env",
                side_effect=fake_resolver,
            ) as resolve,
            patch.dict(os.environ, {"ADAPTER_TEST_SENTINEL": "preserve"}, clear=True),
            patch.object(adapter.os, "execvpe", side_effect=fake_execvpe),
        ):
            result = adapter.main(["cargo", "build", "--locked"])

        self.assertEqual(result, 0)
        configure_trust.assert_called_once_with()
        resolve.assert_called_once()
        self.assertIs(resolve.call_args.args[0], adapter.TARGET_SPECS[target])
        self.assertEqual(resolver_input, {"ADAPTER_TEST_SENTINEL": "preserve"})
        self.assertEqual(captured["command"], "cargo")
        self.assertEqual(captured["argv"], ["cargo", "build", "--locked"])
        environment = captured["environment"]
        assert isinstance(environment, dict)
        self.assertEqual(environment["ADAPTER_TEST_SENTINEL"], "preserve")
        self.assertEqual(environment["RUSTY_V8_ARCHIVE"], "/tmp/v8/archive.a.gz")
        self.assertEqual(environment["RUSTY_V8_SRC_BINDING_PATH"], "/tmp/v8/binding.rs")

    def test_macos_uses_pip_vendored_native_trust_store(self) -> None:
        vendored_truststore = Mock()

        def fake_import(name: str):
            if name == "truststore":
                raise ImportError
            if name == "pip._vendor.truststore":
                return vendored_truststore
            raise AssertionError(name)

        with (
            patch.object(adapter.sys, "platform", "darwin"),
            patch.object(adapter, "import_module", side_effect=fake_import),
        ):
            adapter.configure_native_certificate_store()

        vendored_truststore.inject_into_ssl.assert_called_once_with()

    def test_official_source_build_path_does_not_add_v8_values(self) -> None:
        captured: dict[str, object] = {}

        def fake_execvpe(command, argv, environment):
            captured.update(command=command, argv=argv, environment=environment)

        with (
            patch.dict(os.environ, {"V8_FROM_SOURCE": "1"}, clear=True),
            patch.object(adapter.os, "execvpe", side_effect=fake_execvpe),
        ):
            result = adapter.main(["cargo", "check"])

        self.assertEqual(result, 0)
        environment = captured["environment"]
        assert isinstance(environment, dict)
        self.assertEqual(environment, {"V8_FROM_SOURCE": "1"})

    def test_resolver_failure_does_not_execute_cargo(self) -> None:
        with (
            patch.object(
                adapter,
                "resolve_codex_v8_cargo_env",
                side_effect=RuntimeError("resolver failed"),
            ),
            patch.object(adapter.os, "execvpe") as execvpe,
        ):
            result = adapter.main(["cargo", "build"])

        self.assertEqual(result, 1)
        execvpe.assert_not_called()

    def test_cargo_exit_code_is_transparent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory_path = Path(directory)
            fake_cargo = directory_path / "fake-cargo"
            observed_args = directory_path / "args"
            fake_cargo.write_text(
                "#!/usr/bin/env bash\n"
                "printf '%s\\n' \"$@\" >\"$OBSERVED_ARGS\"\n"
                "exit 37\n",
                encoding="utf-8",
            )
            fake_cargo.chmod(0o755)
            environment = os.environ.copy()
            environment.update(V8_FROM_SOURCE="1", OBSERVED_ARGS=str(observed_args))
            completed = subprocess.run(
                [sys.executable, str(ADAPTER_PATH), str(fake_cargo), "build", "--locked"],
                env=environment,
                check=False,
            )
            observed = observed_args.read_text(encoding="utf-8").splitlines()

        self.assertEqual(completed.returncode, 37)
        self.assertEqual(observed, ["build", "--locked"])


if __name__ == "__main__":
    unittest.main()

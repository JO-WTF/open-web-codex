from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest
from open_web_codex_provider import (
    ProviderContractError,
    ResourceRef,
    ResourceStore,
    resource_ref,
    workspace_resource_root,
)

SERVER_NAME = "supply_chain"
URI_PREFIX = "supply-chain://resources/"


def test_resource_store_is_content_addressed(tmp_path) -> None:
    store = ResourceStore(tmp_path, uri_prefix=URI_PREFIX)

    first = store.publish("example.v1", {"value": 1})
    second = store.publish("example.v1", {"value": 1})

    assert first.resource_id == second.resource_id
    assert json.loads(store.read(first.resource_id)) == {"value": 1}
    assert store.load(resource_ref(first, SERVER_NAME)) == {"value": 1}


def test_resource_store_rejects_path_traversal(tmp_path) -> None:
    store = ResourceStore(tmp_path, uri_prefix=URI_PREFIX)

    with pytest.raises(ValueError, match="invalid"):
        store.read("../secret")


def test_workspace_store_is_visible_across_process_and_restart(tmp_path: Path) -> None:
    profile = tmp_path / "profile"
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    subprocess.run(["git", "init", "-q"], cwd=workspace, check=True)
    root = workspace_resource_root(profile, workspace, SERVER_NAME)
    assert root.is_relative_to(profile / ".open-web-codex")
    assert not root.is_relative_to(workspace)
    package_root = Path(__file__).resolve().parents[1]
    script = """
from pathlib import Path
import sys
from open_web_codex_provider import ResourceStore
published = ResourceStore(
    Path(sys.argv[1]),
    uri_prefix="supply-chain://resources/",
).publish("source_profile.v1", {"ready": True})
print(published.uri)
"""
    completed = subprocess.run(
        [sys.executable, "-c", script, str(root)],
        cwd=package_root,
        check=True,
        capture_output=True,
        text=True,
    )
    uri = completed.stdout.strip()

    restarted = ResourceStore(
        workspace_resource_root(profile, workspace, SERVER_NAME),
        uri_prefix=URI_PREFIX,
    )
    assert restarted.load(
        ResourceRef(
            server=SERVER_NAME,
            uri=uri,
            resource_schema="source_profile.v1",
        )
    ) == {
        "ready": True
    }
    status = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=workspace,
        check=True,
        capture_output=True,
        text=True,
    )
    assert status.stdout == ""


def test_workspace_store_namespace_is_explicit_and_private(tmp_path: Path) -> None:
    profile = tmp_path / "profile"
    workspace = tmp_path / "workspace"
    workspace.mkdir()

    first = workspace_resource_root(profile, workspace, "provider_one")
    second = workspace_resource_root(profile, workspace, "provider_two")

    assert first != second
    assert first.is_relative_to(profile / ".open-web-codex" / "mcp-state")
    assert not first.is_relative_to(workspace)
    with pytest.raises(ValueError, match="provider_namespace_invalid"):
        workspace_resource_root(profile, workspace, "../escape")


def test_workspace_store_denies_same_uri_from_another_workspace(tmp_path: Path) -> None:
    profile = tmp_path / "profile"
    first_workspace = tmp_path / "first"
    second_workspace = tmp_path / "second"
    first_workspace.mkdir()
    second_workspace.mkdir()
    first = ResourceStore(
        workspace_resource_root(profile, first_workspace, SERVER_NAME),
        uri_prefix=URI_PREFIX,
    )
    published = first.publish("source_profile.v1", {"ready": True})
    ref = resource_ref(published, SERVER_NAME)

    second = ResourceStore(
        workspace_resource_root(profile, second_workspace, SERVER_NAME),
        uri_prefix=URI_PREFIX,
    )
    with pytest.raises(ProviderContractError, match="resource_read_invalid"):
        second.load(ref)


def test_resource_store_rejects_forged_schema(tmp_path: Path) -> None:
    store = ResourceStore(tmp_path, uri_prefix=URI_PREFIX)
    published = store.publish("source_profile.v1", {"ready": True})
    forged = ResourceRef(
        server=SERVER_NAME,
        uri=published.uri,
        resource_schema="normalized_network_input.v1",
    )

    with pytest.raises(ValueError, match="schema"):
        store.load(forged)

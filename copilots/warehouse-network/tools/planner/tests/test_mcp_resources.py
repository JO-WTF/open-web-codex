from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

import pytest
from open_web_codex_provider import (
    McpResourceRuntime,
    ProviderContractError,
    ResourceRef,
    ResourceStore,
    derive_geojson_profile,
)
from pydantic import BaseModel, ConfigDict, Field
from supply_chain_planner.delivery.map_service import MapResourceRef

McpResourceContractError = ProviderContractError

SERVER_NAME = "supply_chain"
URI_PREFIX = "supply-chain://resources/"


class ExampleResource(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)

    schema_version: str = Field(alias="schemaVersion")
    value: int
    note: str = ""


def _runtime(workspace: Path, store: ResourceStore) -> McpResourceRuntime:
    return McpResourceRuntime(
        workspace,
        workspace / "profile",
        SERVER_NAME,
        URI_PREFIX,
        store=store,
    )


def _context(sandbox_cwd: str | None) -> SimpleNamespace:
    model_extra = (
        {}
        if sandbox_cwd is None
        else {"codex/sandbox-state-meta": {"sandboxCwd": sandbox_cwd}}
    )
    return SimpleNamespace(
        request_context=SimpleNamespace(meta=SimpleNamespace(model_extra=model_extra))
    )


def test_runtime_publishes_and_strictly_loads_model(tmp_path: Path) -> None:
    runtime = _runtime(
        tmp_path,
        ResourceStore(tmp_path / "resources", uri_prefix=URI_PREFIX),
    )

    result = runtime.publish(
        "example.v1",
        ExampleResource(schemaVersion="example.v1", value=7),
        "Published example",
    )

    assert result.structuredContent is not None
    assert set(result.structuredContent) == {"summary", "resource_ref"}
    ref = ResourceRef.model_validate(result.structuredContent["resource_ref"])
    assert runtime.load_model(ref, "example.v1", ExampleResource).value == 7
    assert json.loads(runtime.read(ref.uri.removeprefix(URI_PREFIX)))["value"] == 7


def test_runtime_publishes_generic_reference_fields_without_workspace_file(tmp_path: Path) -> None:
    runtime = _runtime(
        tmp_path,
        ResourceStore(tmp_path / "resources", uri_prefix=URI_PREFIX),
    )
    map_ref = MapResourceRef(
        server=SERVER_NAME,
        uri="supply-chain://resources/example_geojson.v1-exact",
        profile=derive_geojson_profile({"type": "FeatureCollection", "features": []}),
    )
    result = runtime.publish(
        "example.v1",
        ExampleResource(schemaVersion="example.v1", value=7),
        "Prepared map data",
        mime_type="application/geo+json",
        reference_fields={"data_ref": map_ref},
    )

    assert result.structuredContent is not None
    assert set(result.structuredContent) == {"summary", "resource_ref", "data_ref"}
    resource_ref = ResourceRef.model_validate(
        result.structuredContent["resource_ref"]
    )
    data_ref = MapResourceRef.model_validate(result.structuredContent["data_ref"])
    assert data_ref.server == resource_ref.server == SERVER_NAME
    assert data_ref.format == "geojson"
    assert result.content[1].mimeType == "application/geo+json"
    assert data_ref == map_ref


def test_runtime_rejects_forged_server_schema_and_payload(tmp_path: Path) -> None:
    store = ResourceStore(tmp_path / "resources", uri_prefix=URI_PREFIX)
    runtime = _runtime(tmp_path, store)
    published = store.publish("example.v1", {"schemaVersion": "wrong.v1", "value": 1})

    with pytest.raises(McpResourceContractError, match="resource_server_mismatch"):
        runtime.load_model(
            ResourceRef.model_construct(
                type="mcp_resource",
                server="other",
                uri=published.uri,
                resource_schema="example.v1",
            ),
            "example.v1",
            ExampleResource,
        )
    with pytest.raises(McpResourceContractError, match="resource_uri_mismatch"):
        runtime.load_model(
            ResourceRef(
                server=SERVER_NAME,
                uri="other-provider://resources/example.v1-test",
                resource_schema="example.v1",
            ),
            "example.v1",
            ExampleResource,
        )
    with pytest.raises(McpResourceContractError, match="resource_schema_mismatch"):
        runtime.load_model(
            ResourceRef(
                server=SERVER_NAME,
                uri=published.uri,
                resource_schema="other.v1",
            ),
            "example.v1",
            ExampleResource,
        )
    with pytest.raises(McpResourceContractError, match="resource_payload_schema_mismatch"):
        runtime.load_model(
            ResourceRef(
                server=SERVER_NAME,
                uri=published.uri,
                resource_schema="example.v1",
            ),
            "example.v1",
            ExampleResource,
        )


def test_runtime_enforces_publish_read_and_load_size_bounds(
    tmp_path: Path, monkeypatch
) -> None:
    store = ResourceStore(tmp_path / "resources", uri_prefix=URI_PREFIX)
    runtime = _runtime(tmp_path, store)
    resource = ExampleResource(schemaVersion="example.v1", value=1, note="x" * 80)
    encoded = json.dumps(
        resource.model_dump(mode="json", by_alias=True),
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    monkeypatch.setattr(
        "open_web_codex_provider.runtime.MAX_RESOURCE_BYTES", len(encoded)
    )

    result = runtime.publish("example.v1", resource, "Exact bound")
    ref = ResourceRef.model_validate(result.structuredContent["resource_ref"])
    resource_id = ref.uri.removeprefix(URI_PREFIX)
    assert runtime.read(resource_id)
    assert runtime.load_model(ref, "example.v1", ExampleResource) == resource

    monkeypatch.setattr(
        "open_web_codex_provider.runtime.MAX_RESOURCE_BYTES", len(encoded) - 1
    )

    with pytest.raises(McpResourceContractError, match="resource_publish_invalid"):
        runtime.publish("example.v1", resource, "Too large")
    with pytest.raises(McpResourceContractError, match="resource_read_invalid"):
        runtime.read(resource_id)
    with pytest.raises(McpResourceContractError, match="resource_load_invalid"):
        runtime.load_model(ref, "example.v1", ExampleResource)


def test_runtime_accepts_physical_workspace_alias(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    alias = tmp_path / "workspace-alias"
    alias.symlink_to(workspace, target_is_directory=True)
    runtime = _runtime(
        workspace,
        ResourceStore(tmp_path / "resources", uri_prefix=URI_PREFIX),
    )

    assert runtime.require_workspace(_context(alias.as_uri())) == workspace.resolve()

    created = runtime.create_workspace_file(
        _context(alias.as_uri()),
        "result.txt",
        b"ready",
        max_bytes=5,
    )
    assert created.relative_path == "result.txt"
    assert (workspace / "result.txt").read_bytes() == b"ready"


def test_runtime_creates_canonical_workspace_model(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "deliverables").mkdir()
    runtime = _runtime(
        workspace,
        ResourceStore(tmp_path / "resources", uri_prefix=URI_PREFIX),
    )
    model = ExampleResource(schemaVersion="example.v1", value=7, note="café")
    expected = (
        '{"note":"café","schemaVersion":"example.v1","value":7}'.encode()
    )

    created = runtime.create_workspace_model(
        _context(workspace.as_uri()),
        "deliverables/example.json",
        model,
        max_bytes=len(expected),
    )

    assert created.relative_path == "deliverables/example.json"
    assert created.byte_size == len(expected)
    assert (workspace / created.relative_path).read_bytes() == expected


def test_runtime_workspace_model_preserves_scope_and_size_errors(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    other = tmp_path / "other"
    workspace.mkdir()
    other.mkdir()
    runtime = _runtime(
        workspace,
        ResourceStore(tmp_path / "resources", uri_prefix=URI_PREFIX),
    )
    model = ExampleResource(schemaVersion="example.v1", value=7)

    with pytest.raises(McpResourceContractError, match="workspace_scope_mismatch"):
        runtime.create_workspace_model(
            _context(other.as_uri()),
            "example.json",
            model,
            max_bytes=1_024,
        )
    with pytest.raises(McpResourceContractError, match="workspace_file_invalid"):
        runtime.create_workspace_model(
            _context(workspace.as_uri()),
            "example.json",
            model,
            max_bytes=1,
        )



def test_runtime_rejects_different_workspace_and_invalid_metadata(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    other = tmp_path / "other"
    workspace.mkdir()
    other.mkdir()
    runtime = _runtime(
        workspace,
        ResourceStore(tmp_path / "resources", uri_prefix=URI_PREFIX),
    )

    with pytest.raises(McpResourceContractError, match="workspace_scope_mismatch"):
        runtime.require_workspace(_context(other.as_uri()))
    with pytest.raises(McpResourceContractError, match="workspace_scope_mismatch"):
        runtime.create_workspace_file(
            _context(other.as_uri()),
            "result.txt",
            b"blocked",
            max_bytes=16,
        )
    with pytest.raises(McpResourceContractError, match="workspace_scope_invalid"):
        runtime.require_workspace(_context(None))
    with pytest.raises(McpResourceContractError, match="workspace_scope_invalid"):
        runtime.require_workspace(_context("https://example.invalid/workspace"))

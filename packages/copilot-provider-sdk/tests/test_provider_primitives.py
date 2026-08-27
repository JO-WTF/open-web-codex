from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest
from pydantic import BaseModel, ConfigDict, Field

from open_web_codex_provider.codec import canonical_json_bytes, decode_json_object
from open_web_codex_provider.contracts import ResourceRef
from open_web_codex_provider.errors import ProviderContractError, WorkspaceFileError
from open_web_codex_provider.geojson import GeoJsonResourceRef, derive_geojson_profile
from open_web_codex_provider.runtime import McpResourceRuntime
from open_web_codex_provider.store import ResourceStore, resource_ref, workspace_resource_root
from open_web_codex_provider.workspace import (
    SANDBOX_STATE_META_CAPABILITY,
    create_workspace_file,
    ensure_workspace_directory,
    trusted_workspace_root,
)

SERVER_NAME = "example_provider"
URI_PREFIX = "example-provider://resources/"


class ExampleResource(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)

    schema_version: str = Field(alias="schemaVersion")
    value: int


def _context(workspace: Path) -> SimpleNamespace:
    return SimpleNamespace(
        request_context=SimpleNamespace(
            meta=SimpleNamespace(
                model_extra={
                    SANDBOX_STATE_META_CAPABILITY: {"sandboxCwd": workspace.as_uri()}
                }
            )
        )
    )


def test_resource_ref_is_strict_and_bounded() -> None:
    ref = ResourceRef(
        server=SERVER_NAME,
        uri="example-provider://resources/example.v1-abc",
        resource_schema="example.v1",
    )
    assert ref.model_dump(mode="json") == {
        "type": "mcp_resource",
        "server": SERVER_NAME,
        "uri": "example-provider://resources/example.v1-abc",
        "resource_schema": "example.v1",
    }
    with pytest.raises(ValueError):
        ResourceRef.model_validate({**ref.model_dump(), "unexpected": True})


def test_sandbox_state_meta_capability_has_one_public_sdk_owner() -> None:
    assert SANDBOX_STATE_META_CAPABILITY == "codex/sandbox-state-meta"


def test_geojson_resource_ref_carries_the_exact_schema_and_bounded_profile() -> None:
    ref = GeoJsonResourceRef(
        server="network",
        uri="network-data://resources/network-map",
        resource_schema="network_distribution_geojson.v1",
        profile={
            "feature_count": 1,
            "discriminator_property": "kind",
            "feature_types": [
                {
                    "value": "demand",
                    "feature_count": 1,
                    "geometry_types": ["Point"],
                    "properties": {"city_name": "string"},
                }
            ],
        },
    )
    assert ref.resource_schema == "network_distribution_geojson.v1"
    assert ref.profile.feature_types[0].properties == {"city_name": "string"}
    with pytest.raises(ValueError):
        GeoJsonResourceRef.model_validate(
            {**ref.model_dump(mode="json"), "uri": "https://example.com/map.geojson"}
        )


def test_canonical_codec_is_stable_and_bounded() -> None:
    encoded = canonical_json_bytes({"z": 1, "label": "café"})
    assert encoded == '{"label":"café","z":1}'.encode()
    assert decode_json_object(encoded) == {"label": "café", "z": 1}
    with pytest.raises(ProviderContractError, match="resource_payload_too_large"):
        canonical_json_bytes({"value": "large"}, max_bytes=1)
    with pytest.raises(ProviderContractError, match="resource_payload_invalid"):
        canonical_json_bytes({"value": float("nan")})


def test_resource_store_is_content_addressed_and_schema_checked(tmp_path: Path) -> None:
    store = ResourceStore(tmp_path / "resources", uri_prefix=URI_PREFIX)
    first = store.publish("example.v1", {"schemaVersion": "example.v1", "value": 7})
    second = store.publish("example.v1", {"value": 7, "schemaVersion": "example.v1"})
    assert first.resource_id == second.resource_id
    ref = resource_ref(first, SERVER_NAME)
    assert store.load(ref)["value"] == 7
    with pytest.raises(ProviderContractError, match="resource_schema_mismatch"):
        store.load(ref.model_copy(update={"resource_schema": "other.v1"}))
    with pytest.raises(ProviderContractError, match="resource_id_invalid"):
        store.read("../secret")


def test_runtime_owns_exact_workspace_scope_and_typed_publish_load(tmp_path: Path) -> None:
    profile = tmp_path / "profile"
    workspace = tmp_path / "workspace"
    other = tmp_path / "other"
    profile.mkdir()
    workspace.mkdir()
    other.mkdir()
    runtime = McpResourceRuntime(workspace, profile, SERVER_NAME, URI_PREFIX)
    result = runtime.publish(
        "example.v1",
        ExampleResource(schemaVersion="example.v1", value=7),
        "Published example",
    )
    ref = ResourceRef.model_validate(result.structuredContent["resource_ref"])
    assert runtime.load_model(ref, "example.v1", ExampleResource).value == 7
    assert runtime.require_workspace(_context(workspace)) == workspace.resolve()
    with pytest.raises(ProviderContractError, match="workspace_scope_mismatch"):
        runtime.require_workspace(_context(other))


def test_workspace_namespace_is_private_and_create_new_rejects_links(tmp_path: Path) -> None:
    profile = tmp_path / "profile"
    workspace = tmp_path / "workspace"
    profile.mkdir()
    workspace.mkdir()
    nested = workspace / "deliverables"
    nested.mkdir()
    root = workspace_resource_root(profile, workspace, SERVER_NAME)
    assert root.is_relative_to(profile)
    assert not root.is_relative_to(workspace)
    created = create_workspace_file(workspace, "deliverables/result.json", b"{}")
    assert created.byte_size == 2
    with pytest.raises(WorkspaceFileError, match="workspace_file_exists"):
        create_workspace_file(workspace, "deliverables/result.json", b"{}")
    outside = tmp_path / "outside"
    outside.mkdir()
    (workspace / "linked").symlink_to(outside, target_is_directory=True)
    with pytest.raises(WorkspaceFileError, match="workspace_symlink_rejected"):
        create_workspace_file(workspace, "linked/result.json", b"{}")
    assert trusted_workspace_root(_context(workspace).request_context.meta) == workspace.resolve()


def test_workspace_directory_creation_is_bounded_and_rejects_links(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    outside = tmp_path / "outside"
    workspace.mkdir()
    outside.mkdir()

    assert (
        ensure_workspace_directory(workspace, "outputs/warehouse-network/prepared")
        == "outputs/warehouse-network/prepared"
    )
    assert (workspace / "outputs/warehouse-network/prepared").is_dir()
    assert (
        ensure_workspace_directory(workspace, "outputs/warehouse-network/prepared")
        == "outputs/warehouse-network/prepared"
    )

    (workspace / "linked").symlink_to(outside, target_is_directory=True)
    with pytest.raises(WorkspaceFileError, match="workspace_symlink_rejected"):
        ensure_workspace_directory(workspace, "linked/generated")


def test_runtime_rejects_ambiguous_payload_schema(tmp_path: Path) -> None:
    profile = tmp_path / "profile"
    workspace = tmp_path / "workspace"
    profile.mkdir()
    workspace.mkdir()
    runtime = McpResourceRuntime(workspace, profile, SERVER_NAME, URI_PREFIX)
    with pytest.raises(ProviderContractError, match="resource_payload_schema_ambiguous"):
        runtime.publish(
            "example.v1",
            {"schemaVersion": "example.v1", "schema_version": "other.v1"},
            "invalid",
        )


def test_geojson_profile_is_bounded_and_derived_from_actual_features() -> None:
    profile = derive_geojson_profile(
        {
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "geometry": {"type": "Point", "coordinates": [1, 2]},
                    "properties": {
                        "kind": "demand",
                        "city_name": "Alpha",
                        "duration_hours": 8.5,
                        "served": True,
                    },
                },
                {
                    "type": "Feature",
                    "geometry": {"type": "Point", "coordinates": [3, 4]},
                    "properties": {
                        "kind": "warehouse",
                        "warehouse_type": "center",
                    },
                },
            ],
        }
    )

    assert profile.discriminator_property == "kind"
    assert profile.feature_count == 2
    assert [item.value for item in profile.feature_types] == ["demand", "warehouse"]
    demand = profile.feature_types[0]
    assert demand.properties == {
        "city_name": "string",
        "duration_hours": "number",
        "kind": "string",
        "served": "boolean",
    }
    assert demand.boolean_property_counts["served"].true_count == 1
    assert demand.boolean_property_counts["served"].false_count == 0
    assert "Alpha" not in profile.model_dump_json()


def test_geojson_profile_falls_back_to_geometry_without_kind() -> None:
    profile = derive_geojson_profile(
        {
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "geometry": {"type": "LineString", "coordinates": [[1, 2], [3, 4]]},
                    "properties": {"label": "Route 1"},
                }
            ],
        }
    )
    assert profile.discriminator_property is None
    assert profile.feature_types[0].value == "geometry:LineString"


def test_geojson_profile_marks_missing_properties_as_nullable() -> None:
    profile = derive_geojson_profile(
        {
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "geometry": {"type": "Point", "coordinates": [1, 2]},
                    "properties": {"kind": "demand", "duration_hours": 8.5},
                },
                {
                    "type": "Feature",
                    "geometry": {"type": "Point", "coordinates": [3, 4]},
                    "properties": {"kind": "demand"},
                },
            ],
        }
    )

    assert profile.feature_types[0].properties["duration_hours"] == "number?"

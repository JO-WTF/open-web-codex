"""Exercise both supply-chain MCP servers over their real stdio boundary."""

from __future__ import annotations

import asyncio
import hashlib
import json
import os
import shutil
import tempfile
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from pydantic import AnyUrl

ROOT = Path(__file__).resolve().parents[1]
LAUNCHER = ROOT / "bin" / "supply-chain-planner-launcher"
INDONESIA_SOURCE_RELEASE = ROOT / "examples" / "indonesia-tutorial" / "releases" / "1.0.0"
INDONESIA_WORKSPACE_ID = "0198d5b5-7d0f-7a62-8d9a-f6472dbfab11"
INDONESIA_RELEASE_ID = "0198d5b5-7d0f-7a62-8d9a-f6472dbfab12"
INDONESIA_DATASET_ID = "indonesia-warehouse-network-tutorial"
INDONESIA_VERSION = "1.0.0"
SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"
INDONESIA_FILE_CONTRACT = {
    "dataset-manifest.json": ("dataset_manifest", "application/json"),
    "province-boundaries.geojson": (
        "province_boundaries",
        "application/geo+json",
    ),
    "customers.csv.gz": ("customers", "application/gzip"),
    "customer-assignments.csv.gz": (
        "customer_assignments",
        "application/gzip",
    ),
    "warehouses.csv": ("warehouses", "text/csv"),
    "warehouse-links.csv": ("warehouse_links", "text/csv"),
    "candidate-locations.csv": ("candidate_locations", "text/csv"),
    "transport-quotes.csv": ("transport_quotes", "text/csv"),
    "planning-policy.json": ("planning_policy", "application/json"),
    "validation-report.json": ("validation_report", "application/json"),
}


def server_environment(state_root: Path) -> dict[str, str]:
    """Build an isolated environment while preserving the caller's normal process setup."""

    environment = dict(os.environ)
    environment.update(
        {
            "CODEX_HOME": str(state_root / "codex-home"),
            "OPEN_WEB_CODEX_LOG_DIR": str(state_root / "logs"),
            "SUPPLY_CHAIN_DATA_RESOURCE_DIR": str(state_root / "data-resources"),
            "SUPPLY_CHAIN_RESOURCE_DIR": str(state_root / "planning-resources"),
            "SUPPLY_CHAIN_INDONESIA_RESOURCE_DIR": str(state_root / "indonesia-resources"),
        }
    )
    return environment


async def smoke_data_server(environment: dict[str, str]) -> dict[str, object]:
    """Inspect, build, and validate the fixture through the Data MCP."""

    parameters = StdioServerParameters(
        command=str(LAUNCHER),
        args=["--data-server", "--workspace-root", "."],
        cwd=str(ROOT),
        env=environment,
    )
    async with stdio_client(parameters) as streams:
        async with ClientSession(*streams) as session:
            await session.initialize()
            tools = await session.list_tools()
            assert {tool.name for tool in tools.tools} == {
                "list_planning_sources",
                "inspect_planning_source",
                "build_planning_dataset",
                "validate_planning_dataset",
            }

            catalog = await session.call_tool("list_planning_sources", {})
            assert catalog.isError is not True
            assert catalog.structuredContent is not None
            assert catalog.structuredContent["schema_version"] == ("planning_source_catalog.v2")
            assert catalog.structuredContent["truncated"] is False
            assert [source["source_id"] for source in catalog.structuredContent["sources"]] == [
                "indonesia-network-decision",
                "warehouse-network-fixture",
            ]
            indonesia = catalog.structuredContent["sources"][0]
            assert indonesia["market"] == "ID"
            assert indonesia["candidate_facility_count"] == 3
            assert indonesia["route_count"] == 30

            inspection = await session.call_tool(
                "inspect_planning_source",
                {"source_id": "warehouse-network-fixture"},
            )
            assert inspection.isError is not True
            assert inspection.structuredContent is not None
            assert inspection.structuredContent["source_summary"]["demand_units"] == 100

            build = await session.call_tool(
                "build_planning_dataset",
                {"source_id": "warehouse-network-fixture"},
            )
            assert build.isError is not True
            assert build.structuredContent is not None
            assert build.structuredContent["resource_name"].startswith("planning-dataset.v2-")
            resource_ref = build.structuredContent["data_ref"]

            validation = await session.call_tool(
                "validate_planning_dataset",
                {"resource_ref": resource_ref},
            )
            assert validation.isError is not True
            assert validation.structuredContent is not None
            assert validation.structuredContent["valid"] is True
            return resource_ref


async def smoke_planning_server(
    environment: dict[str, str],
    planning_dataset_ref: dict[str, object],
) -> None:
    """Create and validate one snapshot through the Network Planning MCP."""

    parameters = StdioServerParameters(
        command=str(LAUNCHER),
        args=["--workspace-root", "."],
        cwd=str(ROOT),
        env=environment,
    )
    async with stdio_client(parameters) as streams:
        async with ClientSession(*streams) as session:
            await session.initialize()
            tools = await session.list_tools()
            assert {tool.name for tool in tools.tools} == {
                "prepare_network_snapshot",
                "register_route_matrix",
                "evaluate_current_coverage",
                "evaluate_network_scenario",
                "compare_network_scenarios",
                "solve_facility_location",
                "evaluate_financial_case",
                "publish_risk_register",
                "validate_network_resource",
            }

            snapshot = await session.call_tool(
                "prepare_network_snapshot",
                {"source_path": "examples/network-input.json"},
            )
            assert snapshot.isError is not True
            assert snapshot.structuredContent is not None
            assert snapshot.structuredContent["resource_name"].startswith("network_snapshot.v1-")
            resource_ref = snapshot.structuredContent["data_ref"]

            route_payload = json.loads((ROOT / "examples" / "route-matrix-input.json").read_text())
            routes = await session.call_tool(
                "register_route_matrix",
                {
                    "snapshot_ref": resource_ref,
                    "provider": route_payload["provider"],
                    "method": route_payload["method"],
                    "entries": route_payload["entries"],
                },
            )
            assert routes.isError is not True
            assert routes.structuredContent is not None
            route_ref = routes.structuredContent["data_ref"]

            coverage = await session.call_tool(
                "evaluate_current_coverage",
                {
                    "snapshot_ref": resource_ref,
                    "route_matrix_ref": route_ref,
                },
            )
            assert coverage.isError is not True
            assert coverage.structuredContent is not None
            assert coverage.structuredContent["actual_result_resource_name"].startswith(
                "network_scenario_result.v1-"
            )
            assert coverage.structuredContent["optimized_result_resource_name"].startswith(
                "network_scenario_result.v1-"
            )

            location = await session.call_tool(
                "solve_facility_location",
                {
                    "snapshot_ref": resource_ref,
                    "route_matrix_ref": route_ref,
                    "target_coverage_ratio": 0.90,
                },
            )
            assert location.isError is not True
            assert location.structuredContent is not None
            assert location.structuredContent["result_resource_name"].startswith(
                "network_scenario_result.v1-"
            )

            risk = await session.call_tool(
                "publish_risk_register",
                {
                    "decision_scope": "Cross-store evidence validation",
                    "risks": [
                        {
                            "risk_id": "planning-data-quality",
                            "category": "data",
                            "statement": "Planning data quality may affect the decision.",
                            "likelihood": 3,
                            "impact": 4,
                            "mitigation": "Revalidate the planning dataset before approval.",
                            "trigger": "The planning dataset validation becomes invalid.",
                            "evidence_refs": [planning_dataset_ref],
                        }
                    ],
                },
            )
            assert risk.isError is not True, risk.content
            assert risk.structuredContent is not None
            assert risk.structuredContent["resource_name"].startswith("risk_register.v1-")

            validation = await session.call_tool(
                "validate_network_resource",
                {"resource_ref": resource_ref},
            )
            assert validation.isError is not True
            assert validation.structuredContent is not None
            assert validation.structuredContent["valid"] is True


async def smoke_indonesia_server(
    environment: dict[str, str],
    workspace_root: Path,
    release_binding: dict[str, str],
) -> None:
    """Exercise exact Dataset Release access and bounded handoffs over stdio."""

    parameters = StdioServerParameters(
        command=str(LAUNCHER),
        args=["--indonesia-server", "--workspace-root", "."],
        cwd=str(ROOT),
        env=environment,
    )
    trusted_meta = {
        SANDBOX_STATE_META_CAPABILITY: {
            "sandboxCwd": workspace_root.as_uri(),
        }
    }
    wrong_workspace = workspace_root.parent / "wrong-workspace"
    wrong_workspace.mkdir()
    wrong_meta = {
        SANDBOX_STATE_META_CAPABILITY: {
            "sandboxCwd": wrong_workspace.as_uri(),
        }
    }
    async with stdio_client(parameters) as streams:
        async with ClientSession(*streams) as session:
            initialized = await session.initialize()
            assert initialized.capabilities.experimental is not None
            assert SANDBOX_STATE_META_CAPABILITY in (initialized.capabilities.experimental)
            tools = await session.list_tools()
            assert {tool.name for tool in tools.tools} == {
                "inspect_indonesia_dataset_release",
                "evaluate_indonesia_service_baseline",
                "evaluate_indonesia_current_network",
                "evaluate_indonesia_candidate",
                "optimize_indonesia_new_warehouse",
                "prepare_indonesia_network_map",
                "validate_indonesia_resource",
            }

            await expect_tool_error(
                session,
                "inspect_indonesia_dataset_release",
                {"release": release_binding},
                expected="trusted Turn Workspace metadata",
            )
            await expect_tool_error(
                session,
                "inspect_indonesia_dataset_release",
                {"release": release_binding},
                meta=wrong_meta,
                expected="does not match the current authorized Turn",
            )

            inspection = await session.call_tool(
                "inspect_indonesia_dataset_release",
                {"release": release_binding},
                meta=trusted_meta,
            )
            assert_bounded_resource_result(inspection)
            inspection_ref = inspection.structuredContent["data_ref"]
            inspection_payload = await read_json_resource(session, inspection_ref)
            assert inspection_payload["customer_count"] == 240_000
            assert inspection_payload["province_count"] == 38

            service = await session.call_tool(
                "evaluate_indonesia_service_baseline",
                {"inspection_ref": inspection_ref},
                meta=trusted_meta,
            )
            assert_bounded_resource_result(service)
            service_ref = service.structuredContent["data_ref"]
            service_payload = await read_json_resource(session, service_ref)
            assert service_payload["coverage"]["demand_coverage"]["2_day"] > 0.71
            assert "costs" not in service_payload
            assert "warehouses" not in service_payload
            assert "links" not in service_payload

            current = await session.call_tool(
                "evaluate_indonesia_current_network",
                {"inspection_ref": inspection_ref},
                meta=trusted_meta,
            )
            assert_bounded_resource_result(current)
            current_ref = current.structuredContent["data_ref"]
            current_payload = await read_json_resource(session, current_ref)
            assert current_payload["coverage"]["demand_coverage"]["2_day"] > 0.71
            assert current_payload["costs"]["transport_total_idr"] == 110_024_697_400

            candidate = await session.call_tool(
                "evaluate_indonesia_candidate",
                {
                    "inspection_ref": inspection_ref,
                    "candidate_id": "CAN-PONTIANAK",
                    "opening_amortization_years": 5,
                },
                meta=trusted_meta,
            )
            assert_bounded_resource_result(candidate)
            candidate_ref = candidate.structuredContent["data_ref"]

            optimization = await session.call_tool(
                "optimize_indonesia_new_warehouse",
                {
                    "inspection_ref": inspection_ref,
                    "target_service_days": 2,
                    "target_demand_coverage": 0.74,
                    "opening_amortization_years": 5,
                },
                meta=trusted_meta,
            )
            assert optimization.isError is not True, optimization.content
            assert optimization.structuredContent is not None
            assert len(json.dumps(optimization.structuredContent)) < 4_000
            assert optimization.structuredContent["selected_scenario_ref"] is not None
            optimization_links = [
                block for block in optimization.content if block.type == "resource_link"
            ]
            assert [link.name for link in optimization_links] == [
                optimization.structuredContent["resource_name"],
                optimization.structuredContent["selected_scenario_resource_name"],
            ]
            optimization_payload = await read_json_resource(
                session,
                optimization.structuredContent["data_ref"],
            )
            assert optimization_payload["evaluated_candidate_count"] == 20

            map_result = await session.call_tool(
                "prepare_indonesia_network_map",
                {
                    "baseline_ref": current_ref,
                    "candidate_ref": candidate_ref,
                },
            )
            assert map_result.isError is not True, map_result.content
            assert map_result.structuredContent is not None
            assert set(map_result.structuredContent) == {
                "summary",
                "resource_name",
                "data_ref",
                "geojson_resource_name",
                "geojson_ref",
            }
            assert len(json.dumps(map_result.structuredContent)) < 4_000
            map_payload = await read_json_resource(
                session,
                map_result.structuredContent["data_ref"],
            )
            geojson_payload = await read_json_resource(
                session,
                map_result.structuredContent["geojson_ref"],
                expected_mime_type="application/geo+json",
            )
            assert map_payload["feature_count"] == len(geojson_payload["features"])
            assert len(geojson_payload["features"]) < 200
            assert not any(
                feature["properties"].get("customer_id") for feature in geojson_payload["features"]
            )

            validation = await session.call_tool(
                "validate_indonesia_resource",
                {"resource_ref": map_result.structuredContent["data_ref"]},
            )
            assert validation.isError is not True, validation.content
            assert validation.structuredContent is not None
            assert validation.structuredContent["valid"] is True


async def smoke() -> None:
    """Run all isolated server checks using one temporary state root."""

    with tempfile.TemporaryDirectory(prefix="supply-chain-mcp-smoke-") as directory:
        state_root = Path(directory)
        environment = server_environment(state_root)
        workspace_root, release_binding = prepare_indonesia_release(state_root / "workspaces")
        planning_dataset_ref = await smoke_data_server(environment)
        await smoke_planning_server(environment, planning_dataset_ref)
        await smoke_indonesia_server(
            environment,
            workspace_root,
            release_binding,
        )


def prepare_indonesia_release(
    workspace_parent: Path,
) -> tuple[Path, dict[str, str]]:
    """Build the same immutable on-disk contract written by the Web platform."""

    workspace_root = workspace_parent / INDONESIA_WORKSPACE_ID
    files_root = workspace_root / "datasets" / INDONESIA_DATASET_ID / INDONESIA_VERSION / "files"
    files_root.mkdir(parents=True)
    descriptors = []
    for name, (role, media_type) in sorted(INDONESIA_FILE_CONTRACT.items()):
        target = files_root / name
        shutil.copy2(INDONESIA_SOURCE_RELEASE / name, target)
        descriptors.append(
            {
                "logicalName": name,
                "role": role,
                "mediaType": media_type,
                "byteSize": target.stat().st_size,
                "contentSha256": hashlib.sha256(target.read_bytes()).hexdigest(),
                "relativePath": f"files/{name}",
            }
        )

    display_name = "Indonesia Warehouse Network Tutorial"
    description = "Deterministic synthetic tutorial data."
    digest = hashlib.sha256()
    for value in (
        "workspace.dataset-release.v1",
        INDONESIA_DATASET_ID,
        INDONESIA_VERSION,
        display_name,
        description,
    ):
        update_digest(digest, value)
    for descriptor in descriptors:
        for value in (
            descriptor["logicalName"],
            descriptor["role"],
            descriptor["mediaType"],
            str(descriptor["byteSize"]),
            descriptor["contentSha256"],
        ):
            update_digest(digest, value)
    content_sha256 = digest.hexdigest()
    release_manifest = {
        "schemaVersion": "workspace.dataset-release.v1",
        "releaseId": INDONESIA_RELEASE_ID,
        "datasetId": INDONESIA_DATASET_ID,
        "version": INDONESIA_VERSION,
        "displayName": display_name,
        "description": description,
        "contentSha256": content_sha256,
        "files": descriptors,
    }
    (files_root.parent / "release.json").write_text(
        json.dumps(release_manifest, indent=2) + "\n",
        encoding="utf-8",
    )
    return workspace_root, {
        "workspace_id": INDONESIA_WORKSPACE_ID,
        "release_id": INDONESIA_RELEASE_ID,
        "dataset_id": INDONESIA_DATASET_ID,
        "version": INDONESIA_VERSION,
        "content_sha256": content_sha256,
    }


async def expect_tool_error(
    session: ClientSession,
    name: str,
    arguments: dict[str, object],
    *,
    expected: str,
    meta: dict[str, object] | None = None,
) -> None:
    result = await session.call_tool(name, arguments, meta=meta)
    assert result.isError is True
    text = "\n".join(item.text for item in result.content if getattr(item, "type", None) == "text")
    assert expected in text


def assert_bounded_resource_result(result: object) -> None:
    assert result.isError is not True, result.content
    assert result.structuredContent is not None
    assert set(result.structuredContent) == {
        "summary",
        "resource_name",
        "data_ref",
    }
    assert len(json.dumps(result.structuredContent)) < 4_000


async def read_json_resource(
    session: ClientSession,
    resource_ref: dict[str, object],
    *,
    expected_mime_type: str = "application/json",
) -> dict[str, object]:
    result = await session.read_resource(AnyUrl(str(resource_ref["uri"])))
    assert len(result.contents) == 1
    content = result.contents[0]
    assert content.mimeType == expected_mime_type
    return json.loads(content.text)


def update_digest(digest: hashlib._Hash, value: str) -> None:
    encoded = value.encode("utf-8")
    digest.update(len(encoded).to_bytes(8, "big"))
    digest.update(encoded)


if __name__ == "__main__":
    asyncio.run(smoke())
    print("Supply-chain MCP stdio smoke passed")

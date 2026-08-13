"""Real stdio closure for the composable warehouse-network domain tools.

This is a test harness, not a production workflow. It proves a close-only S3
before an S2 facility optimization while reusing the same exact Resources.
"""

from __future__ import annotations

import asyncio
import json
import os
import sys
import tempfile
from pathlib import Path

import pytest
from _network_fixtures import (
    indonesia_current_assignments,
    indonesia_network_fixture,
    indonesia_provided_route_facts,
    indonesia_route_quotes,
)
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from pydantic import AnyUrl

ROOT = Path(__file__).resolve().parents[1]
SANDBOX_META = "codex/sandbox-state-meta"
BEKASI_ID = "WH-CROSS_DOCKING-BEKASI"
EXPECTED_OPENED = ["WH-CANDIDATE-KENDARI", "WH-CANDIDATE-MANADO"]


def _environment(state_root: Path) -> dict[str, str]:
    environment = dict(os.environ)
    environment.update(
        {
            "CODEX_HOME": str(state_root / "codex-home"),
            "OPEN_WEB_CODEX_DATA_DIR": str(state_root / "runtime"),
            "PYTHONDONTWRITEBYTECODE": "1",
        }
    )
    return environment


def _meta(workspace: Path) -> dict[str, object]:
    return {SANDBOX_META: {"sandboxCwd": workspace.as_uri()}}


async def _call(
    session: ClientSession,
    name: str,
    arguments: dict[str, object],
    workspace: Path,
):
    result = await asyncio.wait_for(
        session.call_tool(name, arguments, meta=_meta(workspace)),
        timeout=60,
    )
    assert result.isError is not True, result.content
    assert result.structuredContent is not None
    return result


async def _read_resource(
    session: ClientSession,
    ref: dict[str, object],
) -> dict[str, object]:
    result = await asyncio.wait_for(
        session.read_resource(AnyUrl(str(ref["uri"]))),
        timeout=30,
    )
    assert len(result.contents) == 1
    return json.loads(result.contents[0].text)


def _write_sources(workspace: Path) -> list[str]:
    fixture = indonesia_network_fixture()
    facts = {
        (item.origin_id, item.destination_id, item.layer): item
        for item in indonesia_provided_route_facts()
    }
    sources = {
        "demand.json": [item.model_dump(mode="json") for item in fixture.demand],
        "existing.json": [
            item.model_dump(mode="json") for item in fixture.warehouses if item.is_existing
        ],
        "candidates.json": [
            item.model_dump(mode="json") for item in fixture.warehouses if not item.is_existing
        ],
        "assignments.json": [
            item.model_dump(mode="json") for item in indonesia_current_assignments()
        ],
        "quotes.json": [
            {
                **item.model_dump(mode="json"),
                **facts[(item.origin_id, item.destination_id, item.layer)].model_dump(
                    mode="json",
                    include={
                        "destination_name",
                        "distance_km",
                        "duration_hours",
                    },
                ),
                "method": facts[(item.origin_id, item.destination_id, item.layer)].source_method,
            }
            for item in indonesia_route_quotes()
        ],
    }
    for relative_path, rows in sources.items():
        (workspace / relative_path).write_text(
            json.dumps(rows, ensure_ascii=False),
            encoding="utf-8",
        )
    return list(sources)


def _mappings(*values: tuple[str, str]) -> list[dict[str, str]]:
    return [
        {
            "source_field": field,
            "target_field": field,
            "transform": transform,
        }
        for field, transform in values
    ]


def _confirmed_sources() -> list[dict[str, object]]:
    warehouse_mappings = _mappings(
        ("warehouse_id", "normalize_identifier"),
        ("warehouse_name", "trim"),
        ("warehouse_type", "normalize_warehouse_type"),
        ("city_id", "normalize_identifier"),
        ("city_name", "trim"),
        ("province_id", "normalize_identifier"),
        ("province_name", "trim"),
        ("longitude", "parse_decimal"),
        ("latitude", "parse_decimal"),
        ("upstream_center_id", "normalize_identifier"),
        ("is_fixed", "parse_boolean"),
    )
    return [
        {
            "relative_path": "demand.json",
            "role": "demand",
            "mappings": _mappings(
                ("city_id", "normalize_identifier"),
                ("city_name", "trim"),
                ("province_id", "normalize_identifier"),
                ("province_name", "trim"),
                ("demand_quantity", "parse_integer"),
                ("longitude", "parse_decimal"),
                ("latitude", "parse_decimal"),
            ),
        },
        {
            "relative_path": "existing.json",
            "role": "existing_warehouse",
            "mappings": warehouse_mappings,
        },
        {
            "relative_path": "candidates.json",
            "role": "candidate_warehouse",
            "mappings": warehouse_mappings,
        },
        {
            "relative_path": "assignments.json",
            "role": "current_assignment",
            "mappings": _mappings(
                ("demand_city_id", "normalize_identifier"),
                ("serving_warehouse_id", "normalize_identifier"),
                ("upstream_center_id", "normalize_identifier"),
            ),
        },
        {
            "relative_path": "quotes.json",
            "role": "route_quote",
            "mappings": _mappings(
                ("origin_id", "normalize_identifier"),
                ("destination_id", "normalize_identifier"),
                ("destination_name", "trim"),
                ("layer", "trim"),
                ("distance_km", "parse_decimal"),
                ("duration_hours", "parse_decimal"),
                ("price_per_vehicle", "parse_decimal"),
                ("currency", "trim"),
                ("vehicle_capacity", "parse_decimal"),
                ("method", "trim"),
            ),
        },
    ]


async def _prepare_normalized_resource(
    workspace: Path,
    environment: dict[str, str],
) -> tuple[dict[str, object], list[str]]:
    relative_paths = _write_sources(workspace)
    trace: list[str] = []
    parameters = StdioServerParameters(
        command=sys.executable,
        args=["-m", "supply_chain_planner.data_server"],
        cwd=str(workspace),
        env=environment,
    )
    async with stdio_client(parameters) as streams:
        async with ClientSession(*streams) as session:
            await asyncio.wait_for(session.initialize(), timeout=30)
            inventory = {tool.name for tool in (await session.list_tools()).tools}
            assert inventory == {
                "discover_workspace_sources",
                "inspect_workspace_sources",
                "normalize_network_input",
                "prepare_network_geography",
            }
            trace.append("inspect_workspace_sources")
            profile = await _call(
                session,
                "inspect_workspace_sources",
                {"relative_paths": relative_paths},
                workspace,
            )
            trace.append("normalize_network_input")
            normalized = await _call(
                session,
                "normalize_network_input",
                {
                    "source_profile_ref": profile.structuredContent["resource_ref"],
                    "confirmed_sources": _confirmed_sources(),
                    "country_code": "ID",
                },
                workspace,
            )
            ref = normalized.structuredContent["resource_ref"]
            payload = await _read_resource(session, ref)
            assert payload["state"] == "ready"
            assert len(payload["demand_cities"]) == 50
            assert len(payload["warehouses"]) == 23
            assert len(payload["current_assignments"]) == 50
            assert len(payload["route_quotes"]) == 580
            assert len(payload["provided_route_facts"]) == 580
            return ref, trace


def _cost_policy() -> dict[str, object]:
    return {
        "rules": [
            {
                "layer": layer,
                "currency": "IDR",
                "fixed_cost_per_demand_unit": 40_000,
                "cost_per_km_per_demand_unit": 1_500,
            }
            for layer in ("last_mile", "linehaul")
        ]
    }


async def _run_network_s3_then_s2(
    workspace: Path,
    environment: dict[str, str],
    normalized_ref: dict[str, object],
) -> tuple[list[str], list[str]]:
    parameters = StdioServerParameters(
        command=sys.executable,
        args=["-m", "supply_chain_planner.server"],
        cwd=str(workspace),
        env=environment,
    )
    async with stdio_client(parameters) as streams:
        async with ClientSession(*streams) as session:
            await asyncio.wait_for(session.initialize(), timeout=30)
            inventory = {tool.name for tool in (await session.list_tools()).tools}
            assert inventory == {
                "plan_route_matrix",
                "build_haversine_route_matrix",
                "build_provided_route_matrix",
                "validate_route_matrix",
                "register_navigation_route_matrix",
                "plan_cost_matrix",
                "prepare_network_comparison_map",
                "prepare_network_distribution_map",
                "evaluate_network_baseline",
                "assess_facility_change",
                "evaluate_facility_scenario",
                "solve_p_median",
                "compare_network_scenarios",
                "render_network_comparison_map",
                "publish_network_planning_report",
            }
            prepared = await _read_resource(session, normalized_ref)
            existing_ids = sorted(
                item["warehouse_id"] for item in prepared["warehouses"] if item["is_existing"]
            )
            common_trace: list[str] = []
            common_trace.append("build_provided_route_matrix")
            provided_result = await _call(
                session,
                "build_provided_route_matrix",
                {
                    "normalized_input_ref": normalized_ref,
                    "warehouse_scope": "existing_only",
                },
                workspace,
            )
            provided_ref = provided_result.structuredContent["resource_ref"]
            provided = await _read_resource(session, provided_ref)
            assert len(provided["rows"]) == 556
            assert provided["validation"]["missing_pair_count"] == 0

            common_trace.append("build_haversine_route_matrix")
            routes_result = await _call(
                session,
                "build_haversine_route_matrix",
                {
                    "normalized_input_ref": normalized_ref,
                    "detour_coefficient": 1.2,
                    "average_speed_kph": 42,
                },
                workspace,
            )
            routes_ref = routes_result.structuredContent["resource_ref"]
            routes = await _read_resource(session, routes_ref)
            assert len(routes["rows"]) == 1168
            assert routes["validation"]["computed_pair_count"] == 1168
            assert routes["validation"]["missing_pair_count"] == 0

            common_trace.append("plan_cost_matrix")
            costs_result = await _call(
                session,
                "plan_cost_matrix",
                {
                    "normalized_input_ref": normalized_ref,
                    "warehouse_scope": "all_warehouses",
                    "calculation_policy": _cost_policy(),
                    "route_matrix_ref": routes_ref,
                },
                workspace,
            )
            costs_ref = costs_result.structuredContent["resource_ref"]
            costs = await _read_resource(session, costs_ref)
            assert len(costs["rows"]) == 1168
            assert costs["missing_routes"] == []

            common_trace.append("evaluate_network_baseline")
            baseline_result = await _call(
                session,
                "evaluate_network_baseline",
                {
                    "normalized_input_ref": normalized_ref,
                    "route_matrix_ref": provided_ref,
                    "cost_matrix_ref": costs_ref,
                    "objective": "min_cost",
                    "service_targets": [6, 12, 18],
                    "coverage_mode": "actual_current",
                },
                workspace,
            )
            baseline_ref = baseline_result.structuredContent["resource_ref"]
            baseline = await _read_resource(session, baseline_ref)
            assert baseline["label"] == "actual_current"
            assert baseline["active_warehouse_ids"] == existing_ids
            assert len(baseline["assignment"]["rows"]) == 50
            assert (
                baseline_result.structuredContent["coverage_metrics"][0]["city_coverage_rate"] >= 0
            )
            assert (
                baseline_result.structuredContent["coverage_metrics"][0][
                    "demand_weighted_coverage_rate"
                ]
                >= 0
            )

            s3_trace: list[str] = []
            s3_trace.append("assess_facility_change")
            assessment_result = await _call(
                session,
                "assess_facility_change",
                {
                    "normalized_input_ref": normalized_ref,
                    "route_matrix_ref": provided_ref,
                    "cost_matrix_ref": costs_ref,
                    "before_ref": baseline_ref,
                    "scenario": {
                        "add_warehouse_ids": [],
                        "remove_warehouse_ids": [BEKASI_ID],
                        "relocations": [],
                        "objective": "min_cost",
                        "service_targets": [12],
                    },
                },
                workspace,
            )
            assert assessment_result.structuredContent["active_warehouse_count"] == 10
            scenario_ref = assessment_result.structuredContent["scenario_ref"]
            scenario = await _read_resource(session, scenario_ref)
            assert scenario["active_warehouse_ids"] == [
                item for item in existing_ids if item != BEKASI_ID
            ]
            assert scenario["warehouse_changes"] == {
                "added": [],
                "removed": [BEKASI_ID],
            }
            s3_comparison = await _read_resource(
                session,
                assessment_result.structuredContent["comparison_ref"],
            )
            assert s3_comparison["selected_warehouse_ids"] == []
            assert s3_comparison["removed_warehouse_ids"] == [BEKASI_ID]
            assert len(s3_comparison["affected_city_ids"]) == 50
            assert len(s3_comparison["reassigned_city_ids"]) == 50
            assert set(s3_trace).isdisjoint(
                {
                    "solve_p_median",
                    "prepare_network_comparison_map",
                    "render_network_comparison_map",
                    "publish_network_planning_report",
                }
            )

            s2_trace: list[str] = []
            s2_trace.append("solve_p_median")
            facility_result = await _call(
                session,
                "solve_p_median",
                {
                    "normalized_input_ref": normalized_ref,
                    "route_matrix_ref": routes_ref,
                    "cost_matrix_ref": costs_ref,
                    "number_to_open": 2,
                    "existing_warehouse_policy": {"mode": "keep_all_existing"},
                    "service_targets": [6, 12, 18],
                    "time_limit_seconds": 30,
                },
                workspace,
            )
            facility_ref = facility_result.structuredContent["resource_ref"]
            facility = await _read_resource(session, facility_ref)
            assert facility["status"] == "optimal"
            assert facility["opened_candidate_ids"] == EXPECTED_OPENED
            assert facility["closed_existing_ids"] == []
            assert len(facility["active_warehouse_ids"]) == 13
            assert facility["cost"]["complete"] is True
            assert [item["target_hours"] for item in facility["service"]] == [6, 12, 18]

            s2_trace.append("compare_network_scenarios")
            s2_comparison_result = await _call(
                session,
                "compare_network_scenarios",
                {
                    "before_ref": baseline_ref,
                    "after_ref": facility_ref,
                    "service_targets": [6, 12, 18],
                },
                workspace,
            )
            comparison_ref = s2_comparison_result.structuredContent["resource_ref"]
            comparison = await _read_resource(session, comparison_ref)
            assert comparison["selected_warehouse_ids"] == EXPECTED_OPENED
            assert comparison["removed_warehouse_ids"] == []

            final_refs = {
                "normalized_input_ref": normalized_ref,
                "baseline_ref": baseline_ref,
                "facility_location_ref": facility_ref,
                "comparison_ref": comparison_ref,
            }
            s2_trace.append("prepare_network_comparison_map")
            inline_map_result = await _call(
                session,
                "prepare_network_comparison_map",
                final_refs,
                workspace,
            )
            assert inline_map_result.structuredContent["map_card_handoff"]["tool"] == {
                "server": "map_utils",
                "name": "create_map_card",
            }
            s2_trace.append("render_network_comparison_map")
            map_result = await _call(
                session,
                "render_network_comparison_map",
                {
                    **final_refs,
                    "output_relative_path": "deliverables/sample2-map.json",
                },
                workspace,
            )
            s2_trace.append("publish_network_planning_report")
            report_result = await _call(
                session,
                "publish_network_planning_report",
                {
                    "report_input": {
                        "mode": "comparison",
                        **final_refs,
                    },
                    "output_relative_path": "deliverables/sample2-report.md",
                },
                workspace,
            )
            assert map_result.structuredContent["artifact"]["schema"] == (
                "network_comparison_map_bundle.v1"
            )
            assert report_result.structuredContent["artifact"]["schema"] == (
                "network_planning_report_markdown.v1"
            )
            map_payload = json.loads(
                (workspace / "deliverables/sample2-map.json").read_text(encoding="utf-8")
            )
            report_markdown = (workspace / "deliverables/sample2-report.md").read_text(
                encoding="utf-8"
            )
            assert map_payload["summary"]["feature_count"] == 187
            assert "# 仓网规划结果简报" in report_markdown
            assert "## 时效覆盖" in report_markdown
            assert "结构化计算结果" in report_markdown
            assert report_result.content[0].text == (
                "正式简报已生成：[下载中文 Markdown 简报](deliverables/sample2-report.md)"
            )
            return [*common_trace, *s3_trace], [*common_trace, *s2_trace]


async def smoke() -> None:
    with tempfile.TemporaryDirectory(prefix="warehouse-network-domain-e2e-") as directory:
        state_root = Path(directory)
        workspace = state_root / "workspace"
        workspace.mkdir()
        (workspace / "deliverables").mkdir()
        environment = _environment(state_root)
        normalized_ref, data_trace = await _prepare_normalized_resource(
            workspace,
            environment,
        )
        assert data_trace == ["inspect_workspace_sources", "normalize_network_input"]
        s3_trace, s2_trace = await _run_network_s3_then_s2(
            workspace,
            environment,
            normalized_ref,
        )
        assert s3_trace[-1:] == ["assess_facility_change"]
        assert s2_trace[-4:] == [
            "compare_network_scenarios",
            "prepare_network_comparison_map",
            "render_network_comparison_map",
            "publish_network_planning_report",
        ]


def test_stdio_domain_s3_then_s2() -> None:
    if os.environ.get("RUN_REAL_STDIO_DOMAIN_E2E") != "1":
        pytest.skip("set RUN_REAL_STDIO_DOMAIN_E2E=1 to launch the real stdio closure")
    asyncio.run(asyncio.wait_for(smoke(), timeout=180))


if __name__ == "__main__":
    asyncio.run(asyncio.wait_for(smoke(), timeout=180))

from __future__ import annotations

import asyncio
import json
from pathlib import Path
from types import SimpleNamespace

import pytest
from _network_fixtures import (
    indonesia_current_assignments,
    indonesia_network_fixture,
    indonesia_provided_route_facts,
    indonesia_route_quotes,
)
from pydantic import ValidationError

from supply_chain_planner import server
from supply_chain_planner.map_service import NetworkComparisonMapBundle
from supply_chain_planner.matrix import build_cost_matrix, build_haversine_route_matrix
from supply_chain_planner.matrix_models import CostCalculationPolicy, DemandUnitCostRule
from supply_chain_planner.mcp_contracts import ResourceRef
from supply_chain_planner.mcp_resources import (
    McpResourceContractError,
    McpResourceRuntime,
)
from supply_chain_planner.models import (
    FacilityChangeAssessmentToolResult,
    NetworkBaselineReportInput,
    NetworkBaselineResourceToolResult,
    NetworkComparisonReportInput,
    NetworkFinalArtifactToolResult,
    PreparedNetworkResource,
)
from supply_chain_planner.network_models import RouteQuoteRecord
from supply_chain_planner.optimization_models import (
    AllowExistingWarehouseClosurePolicy,
    AssignmentComparison,
    BaselineResult,
    KeepAllExistingWarehousePolicy,
    PMedianSolution,
    ScenarioResult,
    ScenarioSpec,
    ServiceCoverageConstraint,
)
from supply_chain_planner.report_service import (
    NETWORK_PLANNING_MARKDOWN_MARKER,
    NETWORK_PLANNING_MARKDOWN_SCHEMA,
)
from supply_chain_planner.resource_store import PublishedResource, ResourceStore
from supply_chain_planner.solver import compare_assignments, solve_assignment

BEKASI_ID = "WH-CROSS_DOCKING-BEKASI"


def _resource_ref(published: PublishedResource) -> ResourceRef:
    return ResourceRef(
        server=server.MCP_SERVER_NAME,
        resource_schema=published.schema,
        uri=published.uri,
    )


def _runtime(tmp_path: Path, monkeypatch) -> tuple[Path, ResourceStore]:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    store = ResourceStore(
        tmp_path / "profile" / "resources",
        uri_prefix=server.RESOURCE_URI_PREFIX,
    )
    monkeypatch.setattr(
        server,
        "_mcp_resource_runtime",
        McpResourceRuntime(
            workspace,
            tmp_path / "profile",
            server.MCP_SERVER_NAME,
            "supply-chain://resources/",
            store=store,
        ),
    )
    return workspace, store


def _context(workspace: Path) -> SimpleNamespace:
    meta = SimpleNamespace(
        model_extra={"codex/sandbox-state-meta": {"sandboxCwd": workspace.as_uri()}}
    )
    return SimpleNamespace(request_context=SimpleNamespace(meta=meta))


def _cost_policy() -> CostCalculationPolicy:
    return CostCalculationPolicy(
        rules=[
            DemandUnitCostRule(
                layer=layer,
                currency="IDR",
                fixed_cost_per_demand_unit=40_000,
                cost_per_km_per_demand_unit=1_500,
            )
            for layer in ("last_mile", "linehaul")
        ]
    )


def _published_network(
    store: ResourceStore,
) -> tuple[ResourceRef, ResourceRef, ResourceRef, set[str], set[str]]:
    fixture = indonesia_network_fixture()
    route_quotes = [
        RouteQuoteRecord.model_validate(quote.model_dump(mode="python"))
        for quote in indonesia_route_quotes()
    ]
    prepared = PreparedNetworkResource(
        country_code="ID",
        state="ready",
        demand_cities=fixture.demand,
        warehouses=fixture.warehouses,
        current_assignments=indonesia_current_assignments(),
        route_quotes=route_quotes,
        provided_route_facts=indonesia_provided_route_facts(),
    )
    routes = build_haversine_route_matrix(
        fixture.demand,
        fixture.warehouses,
        detour_coefficient=1.2,
        average_speed_kph=42,
    )
    costs = build_cost_matrix(
        fixture.demand,
        fixture.warehouses,
        route_quotes,
        _cost_policy(),
        routes,
        warehouse_scope="all_warehouses",
    )
    existing_ids = {
        warehouse.warehouse_id for warehouse in fixture.warehouses if warehouse.is_existing
    }
    candidate_ids = {
        warehouse.warehouse_id for warehouse in fixture.warehouses if not warehouse.is_existing
    }
    return (
        _resource_ref(store.publish(prepared.schema_version, prepared)),
        _resource_ref(store.publish(routes.schema_version, routes)),
        _resource_ref(store.publish(costs.schema_version, costs)),
        existing_ids,
        candidate_ids,
    )


def _result_ref(result) -> ResourceRef:
    assert result.structuredContent is not None
    return ResourceRef.model_validate(result.structuredContent["resource_ref"])


def _sample2_resource_refs(
    store: ResourceStore,
    ctx: SimpleNamespace,
) -> tuple[ResourceRef, ResourceRef, ResourceRef, ResourceRef]:
    prepared_ref, route_ref, cost_ref, existing_ids, _candidate_ids = _published_network(store)
    baseline_result = server.evaluate_network_baseline(
        prepared_ref,
        route_ref,
        "min_cost",
        [6, 12, 18],
        "actual_current",
        ctx,
        cost_ref,
    )
    baseline_ref = _result_ref(baseline_result)
    assert baseline_result.structuredContent is not None
    baseline_summary = baseline_result.structuredContent["summary"]
    assert "active warehouses 11" in baseline_summary
    assert "6h city-count=" in baseline_summary
    assert "12h city-count=" in baseline_summary
    typed_baseline = NetworkBaselineResourceToolResult.model_validate(
        baseline_result.structuredContent
    )
    assert [metric.target_hours for metric in typed_baseline.coverage_metrics] == [
        6,
        12,
        18,
    ]
    assert typed_baseline.uncovered_city_count == len(typed_baseline.uncovered_cities)
    facility_ref = _result_ref(
        server.solve_p_median(
            prepared_ref,
            route_ref,
            cost_ref,
            2,
            KeepAllExistingWarehousePolicy(),
            [6, 12, 18],
            30,
            ctx,
        )
    )
    comparison_ref = _result_ref(
        server.compare_network_scenarios(
            before_ref=baseline_ref,
            after_ref=facility_ref,
            service_targets=[6, 12, 18],
            ctx=ctx,
        )
    )
    return prepared_ref, baseline_ref, facility_ref, comparison_ref


def test_network_planning_tools_require_explicit_parameters_and_hide_context() -> None:
    tools = {tool.name: tool for tool in asyncio.run(server.mcp.list_tools())}

    safe_local_tools = {
        "plan_route_matrix",
        "build_haversine_route_matrix",
        "build_provided_route_matrix",
        "validate_route_matrix",
        "register_navigation_route_matrix",
        "plan_cost_matrix",
        "prepare_network_distribution_map",
        "prepare_network_comparison_map",
        "evaluate_network_baseline",
        "assess_facility_change",
        "evaluate_facility_scenario",
        "compare_network_scenarios",
    }
    for name in safe_local_tools:
        annotations = tools[name].annotations
        assert annotations is not None
        assert annotations.model_dump(by_alias=True, exclude_none=True) == {
            "readOnlyHint": False,
            "destructiveHint": False,
            "idempotentHint": True,
            "openWorldHint": False,
        }
    p_median_annotations = tools["solve_p_median"].annotations
    assert p_median_annotations is not None
    assert p_median_annotations.model_dump(by_alias=True, exclude_none=True) == {
        "readOnlyHint": False,
        "destructiveHint": False,
        "idempotentHint": False,
        "openWorldHint": False,
    }
    for name in {"render_network_comparison_map", "publish_network_planning_report"}:
        annotations = tools[name].annotations
        assert annotations is not None
        assert annotations.model_dump(by_alias=True, exclude_none=True) == {
            "readOnlyHint": False,
            "destructiveHint": False,
            "idempotentHint": False,
            "openWorldHint": True,
        }

    baseline = tools["evaluate_network_baseline"].inputSchema
    assert "ctx" not in baseline["properties"]
    assert {
        "normalized_input_ref",
        "route_matrix_ref",
        "objective",
        "service_targets",
        "coverage_mode",
    }.issubset(baseline["required"])

    scenario = tools["evaluate_facility_scenario"].inputSchema
    assert "ctx" not in scenario["properties"]
    assert {"normalized_input_ref", "route_matrix_ref", "scenario"}.issubset(scenario["required"])

    assessment = tools["assess_facility_change"].inputSchema
    assert "ctx" not in assessment["properties"]
    assert {
        "normalized_input_ref",
        "route_matrix_ref",
        "before_ref",
        "scenario",
    }.issubset(assessment["required"])
    assert "cost_matrix_ref" not in assessment["required"]
    assert assessment["properties"]["before_ref"]["$ref"].endswith("/ComparableResourceRef")
    assessment_output = tools["assess_facility_change"].outputSchema
    assert assessment_output["properties"]["coverage"]["maxItems"] == 32
    assert assessment_output["properties"]["affected_city_ids"]["maxItems"] == 100
    assert assessment_output["properties"]["reassigned_city_ids"]["maxItems"] == 100

    p_median = tools["solve_p_median"].inputSchema
    assert "ctx" not in p_median["properties"]
    assert {
        "normalized_input_ref",
        "route_matrix_ref",
        "cost_matrix_ref",
        "number_to_open",
        "existing_warehouse_policy",
        "service_targets",
        "time_limit_seconds",
    }.issubset(p_median["required"])
    assert "service_constraints" not in p_median["required"]

    comparison = tools["compare_network_scenarios"].inputSchema
    assert "ctx" not in comparison["properties"]
    assert {"before_ref", "after_ref", "service_targets"}.issubset(comparison["required"])
    assert "baseline_ref" not in comparison["properties"]
    assert "candidate_ref" not in comparison["properties"]
    assert comparison["properties"]["before_ref"] == comparison["properties"]["after_ref"]
    comparable_ref_schema = comparison["$defs"]["ComparableResourceRef"]
    assert comparable_ref_schema["properties"]["resource_schema"]["enum"] == [
        "network_baseline.v2",
        "network_scenario.v2",
        "facility_location_solution.v3",
    ]
    assert (
        "Comparable network result schema"
        in comparable_ref_schema["properties"]["resource_schema"]["description"]
    )

    route_plan = tools["plan_route_matrix"].inputSchema
    assert "Route method" in route_plan["properties"]["route_method"]["description"]
    assert (
        "both detour_coefficient and average_speed_kph"
        in route_plan["properties"]["route_method"]["description"]
    )
    assert (
        "Required when route_method is haversine"
        in route_plan["properties"]["detour_coefficient"]["description"]
    )
    assert (
        "Required when route_method is haversine"
        in route_plan["properties"]["average_speed_kph"]["description"]
    )

    assert (
        "Count of candidate new warehouses selected"
        in p_median["properties"]["number_to_open"]["description"]
    )
    assert "keep_all_existing" in p_median["properties"]["existing_warehouse_policy"]["description"]
    assert "allow_closure" in p_median["properties"]["existing_warehouse_policy"]["description"]
    assert "fixed_existing_ids" not in p_median["properties"]
    assert "optional_existing_ids" not in p_median["properties"]
    comparison_map = tools["prepare_network_comparison_map"].inputSchema
    assert "ctx" not in comparison_map["properties"]
    assert set(comparison_map["required"]) == {
        "normalized_input_ref",
        "baseline_ref",
        "facility_location_ref",
        "comparison_ref",
    }
    assert "same exact result" in comparison_map["properties"]["comparison_ref"]["description"]

    final_required = {
        "normalized_input_ref",
        "baseline_ref",
        "facility_location_ref",
        "comparison_ref",
        "output_relative_path",
    }
    final_schema = tools["render_network_comparison_map"].inputSchema
    assert "ctx" not in final_schema["properties"]
    assert set(final_schema["required"]) == final_required
    report_schema = tools["publish_network_planning_report"].inputSchema
    assert "ctx" not in report_schema["properties"]
    assert set(report_schema["required"]) == {"report_input", "output_relative_path"}


def test_facility_scenario_rejects_more_than_32_service_targets_before_loading() -> None:
    unused_ref = ResourceRef(
        server=server.MCP_SERVER_NAME,
        uri="supply-chain://resources/not-loaded",
        resource_schema="normalized_network_input.v1",
    )
    with pytest.raises(McpResourceContractError, match="scenario_service_targets_invalid"):
        server._load_facility_scenario_inputs(
            unused_ref,
            unused_ref.model_copy(update={"resource_schema": "route_matrix.v2"}),
            None,
            ScenarioSpec(
                objective="min_time",
                service_targets=[float(target) for target in range(1, 34)],
            ),
        )


def test_s2_creates_inline_comparison_map_and_markdown_report_artifact(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    (workspace / "outputs").mkdir()
    ctx = _context(workspace)
    refs = _sample2_resource_refs(store, ctx)

    def reject_solver_recompute(*_args, **_kwargs):
        raise AssertionError("final delivery must not recompute a solver result")

    monkeypatch.setattr(server, "solve_assignment", reject_solver_recompute)
    monkeypatch.setattr(server, "solve_current_assignment", reject_solver_recompute)
    monkeypatch.setattr(server, "enumerate_p_median", reject_solver_recompute)

    map_result = server.render_network_comparison_map(
        *refs,
        "outputs/network-map.json",
        ctx,
    )
    report_input = NetworkComparisonReportInput(
        normalized_input_ref=refs[0],
        baseline_ref=refs[1],
        facility_location_ref=refs[2],
        comparison_ref=refs[3],
    )
    report_result = server.publish_network_planning_report(
        report_input,
        "outputs/network-report.md",
        ctx,
    )
    inline_map_result = server.prepare_network_comparison_map(*refs, ctx)

    assert map_result.structuredContent is not None
    assert report_result.structuredContent is not None
    map_tool_result = NetworkFinalArtifactToolResult.model_validate(map_result.structuredContent)
    report_tool_result = NetworkFinalArtifactToolResult.model_validate(
        report_result.structuredContent
    )
    assert set(map_result.structuredContent) == {"summary", "artifact"}
    assert set(map_result.structuredContent["artifact"]) == {
        "schema",
        "displayName",
        "mimeType",
        "workspaceRelativePath",
        "byteSize",
    }
    assert map_tool_result.artifact.artifact_schema == ("network_comparison_map_bundle.v1")
    assert report_tool_result.artifact.artifact_schema == (NETWORK_PLANNING_MARKDOWN_SCHEMA)
    assert map_tool_result.artifact.mime_type == "application/json"
    assert report_tool_result.artifact.mime_type == "text/markdown"

    map_path = workspace / map_tool_result.artifact.workspace_relative_path
    report_path = workspace / report_tool_result.artifact.workspace_relative_path
    assert map_path.stat().st_size == map_tool_result.artifact.byte_size
    assert report_path.stat().st_size == report_tool_result.artifact.byte_size
    map_bundle = NetworkComparisonMapBundle.model_validate(
        json.loads(map_path.read_text(encoding="utf-8"))
    )
    report_markdown = report_path.read_text(encoding="utf-8")
    assert map_bundle.summary.feature_count == 187
    assert map_bundle.summary.opened_candidate_ids == [
        "WH-CANDIDATE-KENDARI",
        "WH-CANDIDATE-MANADO",
    ]
    assert report_markdown.startswith("# 仓网规划结果简报\n")
    assert NETWORK_PLANNING_MARKDOWN_MARKER in report_markdown
    assert "## 执行摘要" in report_markdown
    assert "## 时效覆盖" in report_markdown
    assert "基线城市覆盖率" in report_markdown
    assert "方案需求量加权覆盖率" in report_markdown
    assert "WH-CANDIDATE-KENDARI" in report_markdown
    assert "WH-CANDIDATE-MANADO" in report_markdown
    assert "结构化计算结果" in report_markdown
    assert report_result.content[0].text == (
        "正式简报已生成：[下载中文 Markdown 简报](outputs/network-report.md)"
    )

    assert inline_map_result.structuredContent is not None
    assert inline_map_result.structuredContent["feature_count"] == 187
    handoff = inline_map_result.structuredContent["map_card_handoff"]
    assert handoff["schemaVersion"] == "network_comparison_map_card_handoff.v1"
    assert handoff["tool"] == {
        "server": "map_utils",
        "name": "create_map_card",
    }
    assert [layer["id"] for layer in handoff["arguments"]["layers"]][:2] == [
        "baseline-assignments",
        "planned-assignments",
    ]

    with pytest.raises(McpResourceContractError, match="workspace_file_invalid"):
        server.render_network_comparison_map(
            *refs,
            "outputs/network-map.json",
            ctx,
        )
    for invalid_path in (
        (workspace / "absolute.md").as_posix(),
        "../escape.md",
    ):
        with pytest.raises(McpResourceContractError, match="workspace_file_invalid"):
            server.publish_network_planning_report(report_input, invalid_path, ctx)
    outside = tmp_path / "outside"
    outside.mkdir()
    (workspace / "linked").symlink_to(outside, target_is_directory=True)
    with pytest.raises(McpResourceContractError, match="workspace_file_invalid"):
        server.publish_network_planning_report(
            report_input,
            "linked/report.md",
            ctx,
        )
    assert list(outside.iterdir()) == []

    with pytest.raises(
        McpResourceContractError,
        match="report_output_requires_markdown",
    ):
        server.publish_network_planning_report(
            report_input,
            "outputs/not-a-markdown-report.json",
            ctx,
        )


def test_final_artifact_descriptor_rejects_empty_files() -> None:
    with pytest.raises(ValidationError):
        NetworkFinalArtifactToolResult.model_validate(
            {
                "summary": "empty",
                "artifact": {
                    "schema": "network_planning_report_markdown.v1",
                    "displayName": "Report",
                    "mimeType": "text/markdown",
                    "workspaceRelativePath": "report.md",
                    "byteSize": 0,
                },
            }
        )


def test_baseline_assessment_creates_markdown_without_comparison_refs(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    (workspace / "outputs").mkdir()
    ctx = _context(workspace)
    normalized_ref, baseline_ref, _facility_ref, _comparison_ref = _sample2_resource_refs(
        store, ctx
    )

    result = server.publish_network_planning_report(
        NetworkBaselineReportInput(
            normalized_input_ref=normalized_ref,
            baseline_ref=baseline_ref,
        ),
        "outputs/current-network-assessment.md",
        ctx,
    )

    assert result.structuredContent is not None
    artifact = NetworkFinalArtifactToolResult.model_validate(result.structuredContent)
    assert artifact.artifact.artifact_schema == NETWORK_PLANNING_MARKDOWN_SCHEMA
    markdown = (workspace / artifact.artifact.workspace_relative_path).read_text(encoding="utf-8")
    assert result.content[0].text == (
        "正式简报已生成：[下载中文 Markdown 简报](outputs/current-network-assessment.md)"
    )
    assert markdown.startswith("# 当前仓网评估简报\n")
    assert NETWORK_PLANNING_MARKDOWN_MARKER in markdown
    assert "## 时效覆盖" in markdown
    assert "## 未达标城市（按需求量排序）" in markdown
    assert "结构化计算结果" in markdown


def test_s3_reuses_prepared_resources_and_only_closes_bekasi(tmp_path: Path, monkeypatch) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref, route_ref, cost_ref, existing_ids, candidate_ids = _published_network(store)
    baseline_ref = _result_ref(
        server.evaluate_network_baseline(
            prepared_ref,
            route_ref,
            "min_cost",
            [6, 12, 18],
            "actual_current",
            ctx,
            cost_ref,
        )
    )
    baseline = server._runtime().load_model(
        baseline_ref,
        "network_baseline.v2",
        BaselineResult,
    )
    assert set(baseline.active_warehouse_ids) == existing_ids
    assert baseline.label == "actual_current"
    assert [metric.target_hours for metric in baseline.coverage] == [6, 12, 18]

    scenario_result = server.evaluate_facility_scenario(
        prepared_ref,
        route_ref,
        ScenarioSpec(
            remove_warehouse_ids=[BEKASI_ID],
            objective="min_cost",
            service_targets=[12],
        ),
        ctx,
        cost_ref,
    )
    scenario_ref = _result_ref(scenario_result)
    assert scenario_result.structuredContent is not None
    scenario_summary = scenario_result.structuredContent["summary"]
    assert "10 active warehouses" in scenario_summary
    assert f"removed [{BEKASI_ID}]" in scenario_summary
    assert "coverage 12h city-count=" in scenario_summary
    assert "demand-weighted=" in scenario_summary
    scenario = server._runtime().load_model(
        scenario_ref,
        "network_scenario.v2",
        ScenarioResult,
    )
    assert set(scenario.active_warehouse_ids) == existing_ids - {BEKASI_ID}
    assert not set(scenario.active_warehouse_ids) & candidate_ids
    assert scenario.warehouse_changes == {"added": [], "removed": [BEKASI_ID]}
    assert len(scenario.assignment.rows) == 50
    assert scenario.cost is not None and scenario.cost.complete
    assert [metric.target_hours for metric in scenario.service] == [12]

    comparison_result = server.compare_network_scenarios(
        before_ref=baseline_ref,
        after_ref=scenario_ref,
        service_targets=[12],
        ctx=ctx,
    )
    comparison_ref = _result_ref(comparison_result)
    assert comparison_result.structuredContent is not None
    comparison_summary = comparison_result.structuredContent["summary"]
    assert "affected 50, reassigned 50" in comparison_summary
    assert "coverage 12h city-count" in comparison_summary
    assert "demand-weighted" in comparison_summary
    comparison = server._runtime().load_model(
        comparison_ref,
        "network_assignment_comparison.v2",
        AssignmentComparison,
    )
    assert comparison.selected_warehouse_ids == []
    assert comparison.removed_warehouse_ids == [BEKASI_ID]
    assert comparison.requested_service_targets == [12]
    assert comparison.coverage[0].before.total_city_count == 50
    assert comparison.coverage[0].after.total_city_count == 50
    assert comparison.coverage[0].before.total_demand == comparison.coverage[0].after.total_demand
    assert len(comparison.city_changes) == 50
    assert len(comparison.affected_city_ids) == 50
    assert len(comparison.reassigned_city_ids) == 50

    same_scenario_ref = _result_ref(
        server.compare_network_scenarios(
            before_ref=scenario_ref,
            after_ref=scenario_ref,
            service_targets=[12],
            ctx=ctx,
        )
    )
    same_scenario = server._runtime().load_model(
        same_scenario_ref,
        "network_assignment_comparison.v2",
        AssignmentComparison,
    )
    assert same_scenario.selected_warehouse_ids == []
    assert same_scenario.removed_warehouse_ids == []
    assert same_scenario.affected_city_ids == []

    unsupported_ref = ResourceRef(
        server=server.MCP_SERVER_NAME,
        uri=baseline_ref.uri,
        resource_schema="route_matrix.v2",
    )
    with pytest.raises(
        McpResourceContractError,
        match="comparison_subject_schema_invalid",
    ):
        server.compare_network_scenarios(
            before_ref=unsupported_ref,
            after_ref=scenario_ref,
            service_targets=[12],
            ctx=ctx,
        )


def test_assess_facility_change_preserves_selected_candidates_and_matches_manual_compute(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref, route_ref, cost_ref, _existing_ids, _candidate_ids = _published_network(store)
    facility_ref = _result_ref(
        server.solve_p_median(
            prepared_ref,
            route_ref,
            cost_ref,
            2,
            KeepAllExistingWarehousePolicy(),
            [6, 12, 18],
            30,
            ctx,
        )
    )
    facility = server._runtime().load_model(
        facility_ref,
        "facility_location_solution.v3",
        PMedianSolution,
    )
    assert facility.assignment is not None
    assert facility.opened_candidate_ids

    tool_result = server.assess_facility_change(
        normalized_input_ref=prepared_ref,
        route_matrix_ref=route_ref,
        cost_matrix_ref=cost_ref,
        before_ref=server.ComparableResourceRef.model_validate(facility_ref.model_dump()),
        scenario=ScenarioSpec(
            remove_warehouse_ids=[BEKASI_ID],
            objective="min_cost",
            service_targets=[6, 12, 18],
        ),
        ctx=ctx,
    )
    assert tool_result.structuredContent is not None
    bounded = FacilityChangeAssessmentToolResult.model_validate(tool_result.structuredContent)
    assert bounded.active_warehouse_count == len(facility.active_warehouse_ids) - 1
    assert bounded.added_warehouse_ids == []
    assert bounded.removed_warehouse_ids == [BEKASI_ID]
    assert bounded.cost.currency == "IDR"
    assert bounded.cost.complete
    assert len(bounded.affected_city_ids) <= 100
    assert len(bounded.reassigned_city_ids) <= 100

    scenario = server._runtime().load_model(
        bounded.scenario_ref,
        "network_scenario.v2",
        ScenarioResult,
    )
    expected_active_ids = set(facility.active_warehouse_ids) - {BEKASI_ID}
    assert set(facility.opened_candidate_ids) <= set(scenario.active_warehouse_ids)
    assert set(scenario.active_warehouse_ids) == expected_active_ids

    prepared = server._runtime().load_model(
        prepared_ref,
        "normalized_network_input.v1",
        PreparedNetworkResource,
    )
    routes = server._runtime().load_model(
        route_ref,
        "route_matrix.v2",
        server.ComposableRouteMatrix,
    )
    costs = server._runtime().load_model(
        cost_ref,
        "cost_matrix.v2",
        server.CostMatrix,
    )
    manual_assignment = solve_assignment(
        prepared.demand_cities,
        prepared.warehouses,
        routes,
        costs,
        "min_cost",
        expected_active_ids,
    )
    manual_comparison = compare_assignments(
        facility.assignment,
        manual_assignment,
        [6, 12, 18],
        set(facility.active_warehouse_ids),
        expected_active_ids,
    )
    comparison = server._runtime().load_model(
        bounded.comparison_ref,
        "network_assignment_comparison.v2",
        AssignmentComparison,
    )
    assert scenario.assignment == manual_assignment
    assert comparison == manual_comparison
    assert bounded.coverage == comparison.coverage
    assert bounded.affected_city_count == len(comparison.affected_city_ids)
    assert bounded.reassigned_city_count == len(comparison.reassigned_city_ids)

    wrong_server_ref = server.ComparableResourceRef(
        server="another_provider",
        uri=facility_ref.uri,
        resource_schema="facility_location_solution.v3",
    )
    with pytest.raises(McpResourceContractError, match="resource_server_mismatch"):
        server.assess_facility_change(
            prepared_ref,
            route_ref,
            wrong_server_ref,
            ScenarioSpec(
                remove_warehouse_ids=[BEKASI_ID],
                objective="min_time",
                service_targets=[12],
            ),
            ctx,
        )

    with pytest.raises(
        McpResourceContractError,
        match="min_cost_scenario_requires_cost_matrix",
    ):
        server.assess_facility_change(
            prepared_ref,
            route_ref,
            server.ComparableResourceRef.model_validate(facility_ref.model_dump()),
            ScenarioSpec(
                remove_warehouse_ids=[BEKASI_ID],
                objective="min_cost",
                service_targets=[12],
            ),
            ctx,
        )


def test_comparison_accepts_actual_and_optimized_baseline_resources(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref, route_ref, cost_ref, _existing_ids, _candidate_ids = _published_network(store)
    actual_ref = _result_ref(
        server.evaluate_network_baseline(
            prepared_ref,
            route_ref,
            "min_cost",
            [12],
            "actual_current",
            ctx,
            cost_ref,
        )
    )
    optimized_ref = _result_ref(
        server.evaluate_network_baseline(
            prepared_ref,
            route_ref,
            "min_cost",
            [12],
            "optimized_existing_footprint",
            ctx,
            cost_ref,
        )
    )

    comparison_ref = _result_ref(
        server.compare_network_scenarios(
            before_ref=actual_ref,
            after_ref=optimized_ref,
            service_targets=[12],
            ctx=ctx,
        )
    )
    comparison = server._runtime().load_model(
        comparison_ref,
        "network_assignment_comparison.v2",
        AssignmentComparison,
    )

    assert comparison.selected_warehouse_ids == []
    assert comparison.removed_warehouse_ids == []
    assert comparison.requested_service_targets == [12]
    assert len(comparison.city_changes) == 50


def test_s2_opens_exactly_two_candidates_and_compares_actual_baseline(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref, route_ref, cost_ref, existing_ids, _candidate_ids = _published_network(store)
    baseline_result = server.evaluate_network_baseline(
        prepared_ref,
        route_ref,
        "min_cost",
        [6, 12, 18],
        "actual_current",
        ctx,
        cost_ref,
    )
    baseline_ref = _result_ref(baseline_result)
    assert baseline_result.structuredContent is not None
    assert "6h city-count=" in baseline_result.structuredContent["summary"]
    facility_result = server.solve_p_median(
        prepared_ref,
        route_ref,
        cost_ref,
        2,
        KeepAllExistingWarehousePolicy(),
        [6, 12, 18],
        30,
        ctx,
        [
            ServiceCoverageConstraint(
                target_hours=18,
                minimum_coverage=0.5,
            )
        ],
    )
    facility_ref = _result_ref(facility_result)
    assert facility_result.structuredContent is not None
    facility_summary = facility_result.structuredContent["summary"]
    assert "opened [WH-CANDIDATE-KENDARI, WH-CANDIDATE-MANADO]" in facility_summary
    assert "coverage 6h city-count=" in facility_summary
    assert "demand-weighted=" in facility_summary
    facility = server._runtime().load_model(
        facility_ref,
        "facility_location_solution.v3",
        PMedianSolution,
    )
    assert facility.status == "optimal"
    assert set(facility.active_warehouse_ids) == existing_ids | {
        "WH-CANDIDATE-KENDARI",
        "WH-CANDIDATE-MANADO",
    }
    assert facility.opened_candidate_ids == [
        "WH-CANDIDATE-KENDARI",
        "WH-CANDIDATE-MANADO",
    ]
    assert facility.closed_existing_ids == []
    assert facility.assignment is not None and len(facility.assignment.rows) == 50
    assert facility.cost is not None and facility.cost.complete
    assert [metric.target_hours for metric in facility.service] == [6, 12, 18]

    scenario_ref = _result_ref(
        server.evaluate_facility_scenario(
            prepared_ref,
            route_ref,
            ScenarioSpec(
                remove_warehouse_ids=[BEKASI_ID],
                objective="min_cost",
                service_targets=[12],
            ),
            ctx,
            cost_ref,
        )
    )
    facility_to_scenario_ref = _result_ref(
        server.compare_network_scenarios(
            before_ref=facility_ref,
            after_ref=scenario_ref,
            service_targets=[12],
            ctx=ctx,
        )
    )
    facility_to_scenario = server._runtime().load_model(
        facility_to_scenario_ref,
        "network_assignment_comparison.v2",
        AssignmentComparison,
    )
    assert facility_to_scenario.requested_service_targets == [12]
    assert BEKASI_ID in facility_to_scenario.removed_warehouse_ids

    facility_without_assignment_ref = _resource_ref(
        store.publish(
            facility.schema_version,
            facility.model_copy(update={"assignment": None}),
        )
    )
    with pytest.raises(
        McpResourceContractError,
        match="comparable_assignment_unavailable",
    ):
        server.compare_network_scenarios(
            before_ref=facility_without_assignment_ref,
            after_ref=scenario_ref,
            service_targets=[12],
            ctx=ctx,
        )

    comparison_result = server.compare_network_scenarios(
        before_ref=baseline_ref,
        after_ref=facility_ref,
        service_targets=[6, 12, 18],
        ctx=ctx,
    )
    comparison_ref = _result_ref(comparison_result)
    assert comparison_result.structuredContent is not None
    comparison_summary = comparison_result.structuredContent["summary"]
    assert "selected [WH-CANDIDATE-KENDARI, WH-CANDIDATE-MANADO]" in comparison_summary
    assert "coverage 6h city-count" in comparison_summary and "18h" in comparison_summary
    assert "demand-weighted" in comparison_summary
    comparison = server._runtime().load_model(
        comparison_ref,
        "network_assignment_comparison.v2",
        AssignmentComparison,
    )
    assert comparison.selected_warehouse_ids == facility.opened_candidate_ids
    assert comparison.removed_warehouse_ids == []
    assert comparison.requested_service_targets == [6, 12, 18]


def test_baseline_and_p_median_reject_missing_explicit_inputs(tmp_path: Path, monkeypatch) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref, route_ref, cost_ref, existing_ids, candidate_ids = _published_network(store)
    prepared = server._runtime().load_model(
        prepared_ref,
        "normalized_network_input.v1",
        PreparedNetworkResource,
    )
    without_current = prepared.model_copy(update={"current_assignments": []})
    without_current_ref = _resource_ref(
        store.publish(without_current.schema_version, without_current)
    )
    with pytest.raises(McpResourceContractError, match="current_assignments_required"):
        server.evaluate_network_baseline(
            without_current_ref,
            route_ref,
            "min_cost",
            [12],
            "actual_current",
            ctx,
            cost_ref,
        )

    with pytest.raises(ValueError, match="existing_policy_requires_existing_warehouses"):
        server.solve_p_median(
            prepared_ref,
            route_ref,
            cost_ref,
            2,
            AllowExistingWarehouseClosurePolicy(closable_existing_ids=[next(iter(candidate_ids))]),
            [12],
            30,
            ctx,
        )

    with pytest.raises(
        McpResourceContractError,
        match="scenario_active_upstream_required",
    ):
        server.evaluate_facility_scenario(
            prepared_ref,
            route_ref,
            ScenarioSpec(
                remove_warehouse_ids=["WH-CENTER-JAKARTA"],
                objective="min_cost",
                service_targets=[12],
            ),
            ctx,
            cost_ref,
        )

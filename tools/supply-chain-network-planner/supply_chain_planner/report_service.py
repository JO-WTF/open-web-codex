"""Pure network report bundles, Markdown briefs and legacy Case publication."""

from __future__ import annotations

import hashlib
from decimal import Decimal
from pathlib import Path
from typing import TYPE_CHECKING, Literal
from uuid import UUID

from pydantic import Field

from .delivery_models import (
    DeliveryModel,
    ordered_assignment,
    ordered_comparison,
    ordered_cost,
    ordered_metrics,
    validate_baseline_delivery_inputs,
    validate_delivery_inputs,
)
from .network_models import DemandCityRecord, NormalizedInputBatch, WarehouseRecord
from .optimization_models import (
    AssignmentComparison,
    AssignmentResult,
    BaselineResult,
    CostSummary,
    CoverageMetricSummary,
    PMedianSolution,
    ServiceMetric,
)
from .solver import coverage_metrics

if TYPE_CHECKING:
    from .case_repository import CaseRepository
    from .case_types import CaseOperationResult
    from .resource_store import PublishedResource, ResourceStore

ReportSource = Literal["baseline", "scenario", "facility_location"]
NETWORK_PLANNING_MARKDOWN_SCHEMA = "network_planning_report_markdown.v1"
NETWORK_PLANNING_MARKDOWN_MARKER = "<!-- network_planning_report_markdown.v1 -->"
MAX_BRIEF_CITY_CHANGES = 20


class NetworkReportScope(DeliveryModel):
    demand_city_count: int = Field(ge=0)
    warehouse_count: int = Field(ge=0)
    existing_warehouse_count: int = Field(ge=0)
    candidate_warehouse_count: int = Field(ge=0)
    total_demand: Decimal = Field(ge=0)


class NetworkReportEntities(DeliveryModel):
    demand_cities: list[DemandCityRecord]
    warehouses: list[WarehouseRecord]


class NetworkBaselineReport(DeliveryModel):
    label: Literal["actual_current", "optimized_existing_footprint"]
    active_warehouse_ids: list[str]
    assignment: AssignmentResult
    service: list[ServiceMetric]
    coverage: list[CoverageMetricSummary]
    cost: CostSummary | None
    notice_code: str | None


class NetworkFacilityReport(DeliveryModel):
    status: Literal["optimal", "feasible"]
    active_warehouse_ids: list[str]
    opened_candidate_ids: list[str]
    closed_existing_ids: list[str]
    assignment: AssignmentResult
    objective_value: float | None
    cost: CostSummary | None
    best_bound: float | None
    service: list[ServiceMetric]
    optimality: Literal["proven", "feasible_only", "not_available"]
    message: str | None


class NetworkPlanningReportBundle(DeliveryModel):
    schema_version: Literal["network_planning_report_bundle.v1"] = (
        "network_planning_report_bundle.v1"
    )
    kind: Literal["network_planning_report"] = "network_planning_report"
    title: str = "仓网规划结果简报"
    country_code: str = Field(pattern=r"^[A-Z]{2}$")
    scope: NetworkReportScope
    entities: NetworkReportEntities
    baseline: NetworkBaselineReport
    facility: NetworkFacilityReport
    comparison: AssignmentComparison
    notices: list[str]


class NetworkBaselineAssessmentReportBundle(DeliveryModel):
    schema_version: Literal["network_baseline_assessment_report_bundle.v1"] = (
        "network_baseline_assessment_report_bundle.v1"
    )
    kind: Literal["network_baseline_assessment_report"] = (
        "network_baseline_assessment_report"
    )
    title: str = "当前仓网评估简报"
    country_code: str = Field(pattern=r"^[A-Z]{2}$")
    scope: NetworkReportScope
    entities: NetworkReportEntities
    baseline: NetworkBaselineReport
    notices: list[str]


def build_network_baseline_assessment_report_bundle(
    normalized: NormalizedInputBatch,
    baseline: BaselineResult,
    *,
    country_code: str,
) -> NetworkBaselineAssessmentReportBundle:
    """Build a typed single-network assessment source without inventing a plan."""

    validated = validate_baseline_delivery_inputs(normalized, baseline)
    existing_count = sum(
        1 for warehouse in validated.warehouse_by_id.values() if warehouse.is_existing
    )
    notices = [baseline.notice_code] if baseline.notice_code is not None else []
    return NetworkBaselineAssessmentReportBundle(
        country_code=country_code,
        scope=NetworkReportScope(
            demand_city_count=len(validated.demand_by_id),
            warehouse_count=len(validated.warehouse_by_id),
            existing_warehouse_count=existing_count,
            candidate_warehouse_count=len(validated.warehouse_by_id) - existing_count,
            total_demand=baseline.assignment.total_demand,
        ),
        entities=NetworkReportEntities(
            demand_cities=[
                validated.demand_by_id[city_id]
                for city_id in sorted(validated.demand_by_id)
            ],
            warehouses=[
                validated.warehouse_by_id[warehouse_id]
                for warehouse_id in sorted(validated.warehouse_by_id)
            ],
        ),
        baseline=NetworkBaselineReport(
            label=baseline.label,
            active_warehouse_ids=sorted(validated.baseline_active_ids),
            assignment=ordered_assignment(baseline.assignment),
            service=ordered_metrics(baseline.service),
            coverage=sorted(baseline.coverage, key=lambda item: item.target_hours),
            cost=ordered_cost(baseline.cost),
            notice_code=baseline.notice_code,
        ),
        notices=notices,
    )


def build_network_planning_report_bundle(
    normalized: NormalizedInputBatch,
    baseline: BaselineResult,
    facility: PMedianSolution,
    comparison: AssignmentComparison,
    *,
    country_code: str,
) -> NetworkPlanningReportBundle:
    """Build the typed report source without persistence or solver work."""

    validated = validate_delivery_inputs(normalized, baseline, facility, comparison)
    if facility.assignment is None:  # narrowed by validation; retained for typing.
        raise ValueError("delivery_facility_assignment_required")
    notices = sorted(
        {notice for notice in (baseline.notice_code, facility.message) if notice is not None}
    )
    existing_count = sum(
        1 for warehouse in validated.warehouse_by_id.values() if warehouse.is_existing
    )
    return NetworkPlanningReportBundle(
        country_code=country_code,
        scope=NetworkReportScope(
            demand_city_count=len(validated.demand_by_id),
            warehouse_count=len(validated.warehouse_by_id),
            existing_warehouse_count=existing_count,
            candidate_warehouse_count=len(validated.warehouse_by_id) - existing_count,
            total_demand=baseline.assignment.total_demand,
        ),
        entities=NetworkReportEntities(
            demand_cities=[
                validated.demand_by_id[city_id] for city_id in sorted(validated.demand_by_id)
            ],
            warehouses=[
                validated.warehouse_by_id[warehouse_id]
                for warehouse_id in sorted(validated.warehouse_by_id)
            ],
        ),
        baseline=NetworkBaselineReport(
            label=baseline.label,
            active_warehouse_ids=sorted(validated.baseline_active_ids),
            assignment=ordered_assignment(baseline.assignment),
            service=ordered_metrics(baseline.service),
            coverage=sorted(baseline.coverage, key=lambda item: item.target_hours),
            cost=ordered_cost(baseline.cost),
            notice_code=baseline.notice_code,
        ),
        facility=NetworkFacilityReport(
            status=facility.status,
            active_warehouse_ids=sorted(validated.facility_active_ids),
            opened_candidate_ids=sorted(facility.opened_candidate_ids),
            closed_existing_ids=sorted(facility.closed_existing_ids),
            assignment=ordered_assignment(facility.assignment),
            objective_value=facility.objective_value,
            cost=ordered_cost(facility.cost),
            best_bound=facility.best_bound,
            service=ordered_metrics(facility.service),
            optimality=facility.optimality,
            message=facility.message,
        ),
        comparison=ordered_comparison(comparison),
        notices=notices,
    )


def render_network_planning_report_markdown(
    bundle: NetworkPlanningReportBundle,
) -> str:
    """Render a deterministic business brief from one validated report bundle."""

    targets = sorted(
        {
            *bundle.comparison.requested_service_targets,
            *(metric.target_hours for metric in bundle.baseline.service),
            *(metric.target_hours for metric in bundle.facility.service),
        }
    )
    baseline_coverage = {
        metric.target_hours: metric
        for metric in coverage_metrics(bundle.baseline.assignment, targets)
    }
    facility_coverage = {
        metric.target_hours: metric
        for metric in coverage_metrics(bundle.facility.assignment, targets)
    }
    city_by_id = {city.city_id: city for city in bundle.entities.demand_cities}
    changed_cities = sorted(
        (
            change
            for change in bundle.comparison.city_changes
            if change.affected or change.reassigned
        ),
        key=lambda item: item.demand_city_id,
    )

    lines = [
        f"# {bundle.title}",
        "",
        NETWORK_PLANNING_MARKDOWN_MARKER,
        "",
        "## 执行摘要",
        "",
        f"- 国家代码：`{_markdown_inline(bundle.country_code)}`",
        (
            "- 分析范围："
            f"{bundle.scope.demand_city_count:,} 个需求城市、"
            f"{bundle.scope.existing_warehouse_count:,} 个现有仓、"
            f"{bundle.scope.candidate_warehouse_count:,} 个候选仓，"
            f"需求总量 {_format_decimal(bundle.scope.total_demand)}。"
        ),
        (
            "- 选址结果："
            f"{_facility_status_label(bundle.facility.status)}"
            f"（{_optimality_label(bundle.facility.optimality)}），"
            f"共 {len(bundle.facility.active_warehouse_ids):,} 个启用仓。"
        ),
        (
            "- 仓网变动："
            f"新开 {len(bundle.facility.opened_candidate_ids):,} 个仓，"
            f"关闭 {len(bundle.facility.closed_existing_ids):,} 个仓，"
            f"重新分配 {len(bundle.comparison.reassigned_city_ids):,} 个需求城市。"
        ),
        _cost_summary_line(bundle),
        "",
        "## 仓库变动",
        "",
        f"- 新开仓库：{_id_list(bundle.facility.opened_candidate_ids)}",
        f"- 关闭仓库：{_id_list(bundle.facility.closed_existing_ids)}",
        (
            "- 启用仓库数："
            f"变更前 {len(bundle.baseline.active_warehouse_ids):,} 个，"
            f"变更后 {len(bundle.facility.active_warehouse_ids):,} 个。"
        ),
        "",
        "## 时效覆盖",
        "",
        "| 时效目标 | 基线城市覆盖率 | 方案城市覆盖率 | "
        "基线需求量加权覆盖率 | 方案需求量加权覆盖率 | 变化 |",
        "| ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for target in targets:
        before = baseline_coverage[target]
        after = facility_coverage[target]
        lines.append(
            "| "
            f"{target:g} 小时 | "
            f"{_format_rate(before.city_coverage_rate)} | "
            f"{_format_rate(after.city_coverage_rate)} | "
            f"{_format_rate(before.demand_weighted_coverage_rate)} | "
            f"{_format_rate(after.demand_weighted_coverage_rate)} | "
            f"{_format_signed_rate(after.demand_weighted_coverage_rate - before.demand_weighted_coverage_rate)} |"
        )

    lines.extend(
        [
            "",
            "## 受影响的需求城市",
            "",
            (
                f"共有 {len(bundle.comparison.affected_city_ids):,} 个城市受影响，"
                f"其中 {len(bundle.comparison.reassigned_city_ids):,} 个城市重新分配。"
            ),
            "",
        ]
    )
    if changed_cities:
        lines.extend(
            [
                "| 需求城市 | 变更前仓库 | 变更后仓库 | 时长变化 | 单位成本变化 |",
                "| --- | --- | --- | ---: | ---: |",
            ]
        )
        for change in changed_cities[:MAX_BRIEF_CITY_CHANGES]:
            city = city_by_id.get(change.demand_city_id)
            city_label = (
                f"{city.city_name} (`{change.demand_city_id}`)"
                if city is not None
                else f"`{change.demand_city_id}`"
            )
            lines.append(
                "| "
                f"{_markdown_table_cell(city_label)} | "
                f"{_optional_id(change.before_warehouse_id)} | "
                f"{_optional_id(change.after_warehouse_id)} | "
                f"{_format_signed_number(change.duration_hours_delta, 'h')} | "
                f"{_format_signed_number(change.cost_per_unit_delta, '')} |"
            )
        remaining = len(changed_cities) - MAX_BRIEF_CITY_CHANGES
        if remaining > 0:
            lines.extend(
                [
                    "",
                    f"_另有 {remaining:,} 个变更城市保留在结构化计算结果中。_",
                ]
            )
    else:
        lines.append("没有需求城市发生服务仓库变更。")

    lines.extend(
        [
            "",
            "## 成本汇总",
            "",
            "| 情景 | 总成本 | 干线成本 | 末端成本 | 数据是否完整 |",
            "| --- | ---: | ---: | ---: | :---: |",
            _cost_table_row("基线", bundle.baseline.cost),
            _cost_table_row("规划方案", bundle.facility.cost),
            "",
            "## 说明",
            "",
            (
                "- 完整的仓库—需求城市对应关系属于结构化计算结果，"
                "不在本简报中重复列出。"
            ),
        ]
    )
    if bundle.notices:
        lines.extend(f"- 系统提示：{_markdown_inline(notice)}" for notice in bundle.notices)
    else:
        lines.append("- 本次求解没有额外提示。")
    return "\n".join(lines) + "\n"


def render_network_baseline_assessment_markdown(
    bundle: NetworkBaselineAssessmentReportBundle,
) -> str:
    """Render one deterministic current-network assessment as Markdown."""

    city_by_id = {city.city_id: city for city in bundle.entities.demand_cities}
    maximum_target = max(metric.target_hours for metric in bundle.baseline.coverage)
    uncovered = sorted(
        (
            row
            for row in bundle.baseline.assignment.rows
            if row.warehouse_id is None
            or row.duration_hours is None
            or row.duration_hours > maximum_target
        ),
        key=lambda row: (-row.demand_quantity, row.demand_city_id),
    )
    lines = [
        f"# {bundle.title}",
        "",
        NETWORK_PLANNING_MARKDOWN_MARKER,
        "",
        "## 执行摘要",
        "",
        f"- 国家代码：`{_markdown_inline(bundle.country_code)}`",
        (
            "- 分析范围："
            f"{bundle.scope.demand_city_count:,} 个需求城市、"
            f"{bundle.scope.existing_warehouse_count:,} 个现有仓，"
            f"需求总量 {_format_decimal(bundle.scope.total_demand)}。"
        ),
        (
            "- 评估口径："
            f"{_baseline_label(bundle.baseline.label)}，"
            f"采用{_objective_label(bundle.baseline.assignment.objective)}。"
        ),
        "",
        "## 时效覆盖",
        "",
        "| 时效目标 | 覆盖城市数 | 城市覆盖率 | 覆盖需求量 | 需求量加权覆盖率 |",
        "| ---: | ---: | ---: | ---: | ---: |",
    ]
    for metric in bundle.baseline.coverage:
        lines.append(
            "| "
            f"{metric.target_hours:g} 小时 | "
            f"{metric.covered_city_count:,}/{metric.total_city_count:,} | "
            f"{_format_rate(metric.city_coverage_rate)} | "
            f"{_format_decimal(metric.covered_demand)}/{_format_decimal(metric.total_demand)} | "
            f"{_format_rate(metric.demand_weighted_coverage_rate)} |"
        )
    lines.extend(
        [
            "",
            "## 未达标城市（按需求量排序）",
            "",
        ]
    )
    if uncovered:
        lines.extend(
            [
                "| 需求城市 | 服务仓库 | 需求量 | 时长 | 原因 |",
                "| --- | --- | ---: | ---: | --- |",
            ]
        )
        for row in uncovered[:MAX_BRIEF_CITY_CHANGES]:
            city = city_by_id[row.demand_city_id]
            duration = "—" if row.duration_hours is None else f"{row.duration_hours:.1f}h"
            lines.append(
                "| "
                f"{_markdown_table_cell(city.city_name)} (`{_markdown_inline(row.demand_city_id)}`) | "
                f"{_optional_id(row.warehouse_id)} | "
                f"{_format_decimal(row.demand_quantity)} | "
                f"{duration} | "
                f"{_markdown_table_cell(_reason_label(row.reason))} |"
            )
        remaining = len(uncovered) - MAX_BRIEF_CITY_CHANGES
        if remaining > 0:
            lines.extend(
                [
                    "",
                    f"_另有 {remaining:,} 个未达标城市保留在结构化计算结果中。_",
                ]
            )
    else:
        lines.append("全部需求城市均满足所请求的时效目标。")
    lines.extend(
        [
            "",
            "## 成本汇总",
            "",
            "| 情景 | 总成本 | 干线成本 | 末端成本 | 数据是否完整 |",
            "| --- | ---: | ---: | ---: | :---: |",
            _cost_table_row("当前仓网", bundle.baseline.cost),
            "",
            "## 说明",
            "",
            (
                "- 完整的仓库—需求城市对应关系属于结构化计算结果，"
                "不在本简报中重复列出。"
            ),
        ]
    )
    if bundle.notices:
        lines.extend(f"- 系统提示：{_markdown_inline(notice)}" for notice in bundle.notices)
    else:
        lines.append("- 本次评估没有额外提示。")
    return "\n".join(lines) + "\n"


def _cost_summary_line(bundle: NetworkPlanningReportBundle) -> str:
    before = bundle.comparison.before_cost
    after = bundle.comparison.after_cost
    delta = bundle.comparison.cost_delta
    currency = (
        bundle.facility.cost.currency
        if bundle.facility.cost is not None
        else bundle.baseline.cost.currency
        if bundle.baseline.cost is not None
        else None
    )
    if before is None or after is None or delta is None or currency is None:
        return "- 成本对比：当前输入不足，暂不可用。"
    return (
        f"- 成本：变更前 {_format_money(before, currency)}，"
        f"变更后 {_format_money(after, currency)}，"
        f"变化 {_format_signed_money(delta, currency)}。"
    )


def _cost_table_row(label: str, cost: CostSummary | None) -> str:
    if cost is None:
        return f"| {label} | 不可用 | 不可用 | 不可用 | 否 |"
    return (
        f"| {label} | {_format_money(cost.total, cost.currency)} | "
        f"{_format_money(cost.linehaul, cost.currency)} | "
        f"{_format_money(cost.last_mile, cost.currency)} | "
        f"{'是' if cost.complete else '否'} |"
    )


def _baseline_label(value: str) -> str:
    return {
        "actual_current": "实际当前归属",
        "optimized_existing_footprint": "现有仓网优化分配",
    }.get(value, _markdown_inline(value))


def _objective_label(value: str) -> str:
    return {
        "min_time": "时效最优",
        "min_cost": "成本最优",
    }.get(value, _markdown_inline(value))


def _facility_status_label(value: str) -> str:
    return {"optimal": "最优方案", "feasible": "可行方案"}.get(
        value, _markdown_inline(value)
    )


def _optimality_label(value: str) -> str:
    return {
        "proven": "已证明最优",
        "feasible_only": "仅确认可行",
        "not_available": "最优性不可用",
    }.get(value, _markdown_inline(value))


def _reason_label(value: str | None) -> str:
    if value in {None, "outside requested target"}:
        return "超出目标时效"
    return _markdown_inline(value)


def _format_decimal(value: Decimal) -> str:
    return f"{value:,.4f}".rstrip("0").rstrip(".")


def _format_rate(value: float) -> str:
    return f"{value:.1%}"


def _format_signed_rate(value: float) -> str:
    return f"{value:+.1%}"


def _format_money(value: float, currency: str) -> str:
    return f"{_markdown_inline(currency)} {value:,.2f}"


def _format_signed_money(value: float, currency: str) -> str:
    return f"{_markdown_inline(currency)} {value:+,.2f}"


def _format_signed_number(value: float | None, suffix: str) -> str:
    if value is None:
        return "—"
    return f"{value:+,.2f}{suffix}"


def _id_list(values: list[str]) -> str:
    if not values:
        return "无"
    return ", ".join(f"`{_markdown_inline(value)}`" for value in values)


def _optional_id(value: str | None) -> str:
    return "—" if value is None else f"`{_markdown_inline(value)}`"


def _markdown_table_cell(value: str) -> str:
    return _markdown_inline(value).replace("|", "\\|")


def _markdown_inline(value: str) -> str:
    return (
        value.replace("\\", "\\\\")
        .replace("\r", " ")
        .replace("\n", " ")
        .replace("&", "&amp;")
        .replace("`", "\\`")
        .replace("[", "\\[")
        .replace("]", "\\]")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
    )


class NetworkReportService:
    def __init__(self, repository: CaseRepository, resource_store: ResourceStore):
        self.repository = repository
        self.resource_store = resource_store

    def publish(
        self,
        case_id: UUID,
        workspace_root: Path,
        source: ReportSource,
    ) -> tuple[PublishedResource, dict[str, object], CaseOperationResult]:
        from .case_types import FacetName

        case = self.repository.get_case(case_id, workspace_root)
        if source == "baseline":
            value, source_component_id = self.repository.load_baseline(case_id, workspace_root)
            payload = {
                "schema_version": "network_planning_report.v1",
                "case_id": str(case_id),
                "country_code": case.country_code,
                "source": source,
                "result_label": value.label,
                "objective": value.assignment.objective,
                "total_demand": str(value.assignment.total_demand),
                "unassigned_demand": str(value.assignment.unassigned_demand),
                "service_metrics": [metric.model_dump(mode="json") for metric in value.service],
                "cost_summary": (
                    value.cost.model_dump(mode="json") if value.cost is not None else None
                ),
                "notices": [value.notice_code] if value.notice_code else [],
            }
        elif source == "scenario":
            value, source_component_id = self.repository.load_scenario(case_id, workspace_root)
            payload = {
                "schema_version": "network_planning_report.v1",
                "case_id": str(case_id),
                "country_code": case.country_code,
                "source": source,
                "result_label": "scenario",
                "objective": value.assignment.objective,
                "total_demand": str(value.assignment.total_demand),
                "unassigned_demand": str(value.assignment.unassigned_demand),
                "warehouse_changes": value.warehouse_changes,
                "service_metrics": [metric.model_dump(mode="json") for metric in value.service],
                "cost_summary": (
                    value.cost.model_dump(mode="json") if value.cost is not None else None
                ),
                "notices": [],
            }
        else:
            value, source_component_id = self.repository.load_facility_solution(
                case_id, workspace_root
            )
            payload = {
                "schema_version": "network_planning_report.v1",
                "case_id": str(case_id),
                "country_code": case.country_code,
                "source": source,
                "result_label": value.status,
                "objective": (
                    value.assignment.objective if value.assignment is not None else "min_cost"
                ),
                "total_demand": (
                    str(value.assignment.total_demand) if value.assignment is not None else None
                ),
                "unassigned_demand": (
                    str(value.assignment.unassigned_demand)
                    if value.assignment is not None
                    else None
                ),
                "selected_warehouse_ids": value.selected_warehouse_ids,
                "objective_value": value.objective_value,
                "best_bound": value.best_bound,
                "optimality": value.optimality,
                "service_metrics": [metric.model_dump(mode="json") for metric in value.service],
                "notices": [value.message] if value.message else [],
            }

        published = self.resource_store.publish("network_planning_report.v1", payload)
        raw = self.resource_store.read(published.resource_id).encode("utf-8")
        content_sha256 = hashlib.sha256(raw).hexdigest()
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "publish_network_planning_report",
            {
                "source": source,
                "source_component_id": str(source_component_id),
                "content_sha256": content_sha256,
            },
        )
        summary = {
            "source": source,
            "result_label": payload["result_label"],
            "service_metric_count": len(payload["service_metrics"]),
            "has_cost": payload.get("cost_summary") is not None,
        }
        result = self.repository.commit_deliverable(
            lease,
            workspace_root,
            component_kind="network_planning_report.v1",
            facet=FacetName.REPORT,
            artifact_schema="network_planning_report.v1",
            media_type="application/json",
            byte_size=len(raw),
            content_sha256=content_sha256,
            storage_name=published.resource_id,
            summary=summary,
            depends_on=[source_component_id],
        )
        return published, summary, result

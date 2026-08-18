"""Deterministic normalization of selected Workspace mappings into Case rows."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Literal

from pydantic import ValidationError
from supply_chain_planner.data.mapping import SourceRole, TransformRegistry, TransformSpec
from supply_chain_planner.network.models import (
    CurrentAssignmentRecord,
    DataQualityIssue,
    DemandCityRecord,
    NormalizedInputBatch,
    ProvidedRouteFactRecord,
    RouteQuoteRecord,
    WarehouseRecord,
)

NormalizationState = Literal["ready", "needs_input", "needs_geography"]


@dataclass(frozen=True)
class ConfirmedFieldMapping:
    target_field: str
    source_field: str
    transform: TransformSpec


@dataclass(frozen=True)
class ConfirmedSourceRows:
    """Current validated rows plus caller-confirmed domain field mappings."""

    role: SourceRole | str
    rows: Sequence[Mapping[str, object]]
    mappings: Sequence[ConfirmedFieldMapping]


class RowNormalizationError(ValueError):
    def __init__(self, code: str, field_name: str | None = None):
        super().__init__(code)
        self.code = code
        self.field_name = field_name


class NetworkInputValidator:
    def validate(self, batch: NormalizedInputBatch) -> list[DataQualityIssue]:
        issues = list(batch.issues)
        demand_ids = [row.city_id for row in batch.demand_cities]
        warehouse_ids = [row.warehouse_id for row in batch.warehouses]
        if not demand_ids:
            issues.append(self._error("demand_missing", "没有找到可用于规划的需求城市数据。"))
        if not warehouse_ids:
            issues.append(self._error("warehouse_missing", "没有找到已有仓库数据。"))
        if len(set(demand_ids)) != len(demand_ids):
            issues.append(self._error("demand_city_duplicate", "需求城市标识必须唯一。"))
        if len(set(warehouse_ids)) != len(warehouse_ids):
            issues.append(self._error("warehouse_duplicate", "仓库标识必须唯一。"))
        demand_set = set(demand_ids)
        existing_warehouse_set = {
            row.warehouse_id for row in batch.warehouses if row.is_existing
        }
        assignment_ids = [row.demand_city_id for row in batch.current_assignments]
        if len(set(assignment_ids)) != len(assignment_ids):
            issues.append(
                self._error("current_assignment_duplicate", "每个需求城市只能有一个当前服务仓。")
            )
        for row in batch.current_assignments:
            if row.demand_city_id not in demand_set:
                issues.append(
                    self._error(
                        "assignment_demand_unknown",
                        f"当前覆盖中的需求城市 {row.demand_city_id} 不在需求列表中。",
                    )
                )
            if row.serving_warehouse_id not in existing_warehouse_set:
                issues.append(
                    self._error(
                        "assignment_warehouse_unknown",
                        f"当前覆盖中的仓库 {row.serving_warehouse_id} 不在已有仓列表中。",
                    )
                )
        center_ids = {
            row.warehouse_id
            for row in batch.warehouses
            if row.warehouse_type == "center"
        }
        for row in batch.warehouses:
            if (
                row.warehouse_type == "cross_docking"
                and row.upstream_center_id is not None
                and row.upstream_center_id not in center_ids
            ):
                issues.append(
                    self._error(
                        "warehouse_upstream_center_unknown",
                        f"前置仓 {row.warehouse_id} 指定的中心仓 "
                        f"{row.upstream_center_id} 不在仓库列表中。",
                    )
                )
        quote_keys = [
            (row.origin_id, row.destination_id, row.layer) for row in batch.route_quotes
        ]
        if len(set(quote_keys)) != len(quote_keys):
            issues.append(self._error("route_quote_duplicate", "路线报价键必须唯一。"))
        provided_fact_keys = [
            (row.origin_id, row.destination_id, row.layer)
            for row in batch.provided_route_facts
        ]
        if len(set(provided_fact_keys)) != len(provided_fact_keys):
            issues.append(
                self._error("provided_route_fact_duplicate", "已提供路线事实键必须唯一。")
            )
        if not batch.current_assignments:
            issues.append(
                DataQualityIssue(
                    code="current_assignment_missing",
                    severity="warning",
                    business_message="没有发现当前覆盖方案；当前分析将使用优化基线。",
                )
            )
        return issues

    @staticmethod
    def _error(code: str, message: str) -> DataQualityIssue:
        return DataQualityIssue(code=code, severity="error", business_message=message)


def normalize_confirmed_rows(
    sources: Sequence[ConfirmedSourceRows],
) -> tuple[NormalizationState, NormalizedInputBatch]:
    """Normalize verified rows without reading files or persisting Case state."""

    transforms = TransformRegistry()
    demands: list[DemandCityRecord] = []
    warehouses: list[WarehouseRecord] = []
    assignments: list[CurrentAssignmentRecord] = []
    quotes: list[RouteQuoteRecord] = []
    provided_route_facts: list[ProvidedRouteFactRecord] = []
    issues: list[DataQualityIssue] = []
    for source in sources:
        try:
            role = SourceRole(source.role)
        except ValueError:
            issues.append(
                DataQualityIssue(
                    code="normalization_role_unknown",
                    severity="error",
                    business_message="输入包含无法识别的数据角色。",
                    field_name=str(source.role)[:256],
                )
            )
            continue
        if role == SourceRole.ADMINISTRATIVE_CATALOG:
            issues.append(
                DataQualityIssue(
                    code="normalization_role_unsupported",
                    severity="error",
                    business_message="行政区目录应由地理准备工具处理，不能作为仓网记录标准化。",
                    field_name=role.value,
                )
            )
            continue
        for row_index, row in enumerate(source.rows, start=1):
            try:
                values = _map_confirmed_row(row, source.mappings, transforms)
                _append_normalized_record(
                    role,
                    values,
                    demands,
                    warehouses,
                    assignments,
                    quotes,
                    provided_route_facts,
                )
            except RowNormalizationError as error:
                issues.append(
                    DataQualityIssue(
                        code=error.code,
                        severity="error",
                        business_message=(
                            f"{role.value} 数据第 {row_index} 行缺少必需字段或格式不正确。"
                        ),
                        field_name=error.field_name,
                    )
                )
            except (KeyError, ValueError, ValidationError) as error:
                issues.append(
                    DataQualityIssue(
                        code=f"{role.value}_row_invalid",
                        severity="error",
                        business_message=(
                            f"{role.value} 数据第 {row_index} 行缺少必需字段或格式不正确。"
                        ),
                        field_name=str(error)[:256],
                    )
                )
    batch = NormalizedInputBatch(
        demand_cities=demands,
        warehouses=warehouses,
        current_assignments=assignments,
        route_quotes=quotes,
        provided_route_facts=provided_route_facts,
        issues=issues,
    )
    validated = batch.model_copy(update={"issues": NetworkInputValidator().validate(batch)})
    if any(issue.severity == "error" for issue in validated.issues):
        return "needs_input", validated
    if any(
        record.longitude is None or record.latitude is None
        for record in [*validated.demand_cities, *validated.warehouses]
    ):
        return "needs_geography", validated
    return "ready", validated


def _map_confirmed_row(
    row: Mapping[str, object],
    mappings: Sequence[ConfirmedFieldMapping],
    transforms: TransformRegistry,
) -> dict[str, object]:
    values: dict[str, object] = {}
    for mapping in mappings:
        value = row.get(mapping.source_field)
        if value in (None, "") and "[]." in mapping.source_field:
            value = row.get(mapping.source_field.rsplit("[].", 1)[-1])
        if value in (None, ""):
            continue
        values[mapping.target_field] = transforms.apply(mapping.transform, value)
    return values


def _append_normalized_record(
    role: SourceRole,
    values: Mapping[str, object],
    demands: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    assignments: list[CurrentAssignmentRecord],
    quotes: list[RouteQuoteRecord],
    provided_route_facts: list[ProvidedRouteFactRecord],
) -> None:
    if role == SourceRole.DEMAND:
        demands.append(
            DemandCityRecord(
                city_id=values["city_id"],
                city_name=values["city_name"],
                province_id=values.get("province_id"),
                province_name=values.get("province_name"),
                demand_quantity=values["demand_quantity"],
                longitude=values.get("longitude"),
                latitude=values.get("latitude"),
            )
        )
        return
    if role in {SourceRole.EXISTING_WAREHOUSE, SourceRole.CANDIDATE_WAREHOUSE}:
        is_existing = role == SourceRole.EXISTING_WAREHOUSE
        warehouses.append(
            WarehouseRecord(
                warehouse_id=values["warehouse_id"],
                warehouse_name=values["warehouse_name"],
                warehouse_type=values["warehouse_type"],
                city_id=values["city_id"],
                city_name=values["city_name"],
                province_id=values.get("province_id"),
                province_name=values.get("province_name"),
                longitude=values.get("longitude"),
                latitude=values.get("latitude"),
                upstream_center_id=values.get("upstream_center_id"),
                is_existing=is_existing,
                is_fixed=values.get("is_fixed"),
            )
        )
        return
    if role == SourceRole.CURRENT_ASSIGNMENT:
        assignments.append(
            CurrentAssignmentRecord(
                demand_city_id=values["demand_city_id"],
                serving_warehouse_id=values["serving_warehouse_id"],
                upstream_center_id=values.get("upstream_center_id"),
            )
        )
        return
    if role == SourceRole.ROUTE_QUOTE:
        for field_name in ("layer", "currency", "vehicle_capacity"):
            if values.get(field_name) in (None, ""):
                raise RowNormalizationError(
                    f"route_quote_{field_name}_missing",
                    field_name,
                )
        route_fact_fields = ("distance_km", "duration_hours", "method")
        supplied_route_fact_fields = {
            field_name
            for field_name in route_fact_fields
            if values.get(field_name) not in (None, "")
        }
        if supplied_route_fact_fields and supplied_route_fact_fields != set(
            route_fact_fields
        ):
            missing = sorted(set(route_fact_fields) - supplied_route_fact_fields)
            raise RowNormalizationError(
                "provided_route_fact_incomplete",
                ",".join(missing),
            )
        quotes.append(
            RouteQuoteRecord(
                origin_id=values["origin_id"],
                destination_id=values["destination_id"],
                layer=str(values["layer"]).lower(),
                price_per_vehicle=values["price_per_vehicle"],
                currency=str(values["currency"]).upper(),
                vehicle_capacity=values["vehicle_capacity"],
            )
        )
        if supplied_route_fact_fields:
            provided_route_facts.append(
                ProvidedRouteFactRecord(
                    origin_id=values["origin_id"],
                    destination_id=values["destination_id"],
                    destination_name=values.get("destination_name"),
                    layer=str(values["layer"]).lower(),
                    distance_km=values["distance_km"],
                    duration_hours=values["duration_hours"],
                    source_method=str(values["method"]).strip(),
                )
            )
        return
    raise RowNormalizationError("normalization_role_unsupported", role.value)

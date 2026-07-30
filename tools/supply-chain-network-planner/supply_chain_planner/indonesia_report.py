"""Deterministic decision-report rendering from validated Indonesia Resources."""

from __future__ import annotations

import hashlib

from .indonesia_models import (
    IndonesiaCandidateScenario,
    IndonesiaCurrentNetworkAnalysis,
    IndonesiaDatasetInspection,
    IndonesiaDecisionReport,
    IndonesiaDecisionReportSources,
    IndonesiaLocationOptimization,
    IndonesiaNetworkMap,
    IndonesiaServiceBaseline,
)


def build_decision_report(
    *,
    inspection: IndonesiaDatasetInspection,
    service: IndonesiaServiceBaseline,
    current: IndonesiaCurrentNetworkAnalysis,
    optimization: IndonesiaLocationOptimization,
    candidate: IndonesiaCandidateScenario,
    network_map: IndonesiaNetworkMap,
    sources: IndonesiaDecisionReportSources,
) -> IndonesiaDecisionReport:
    """Cross-check source ownership and render only Tool-owned facts."""

    release = inspection.release
    for value in (service, current, optimization, candidate, network_map):
        if value.release != release:
            raise ValueError("Decision-report Resources do not share one Dataset Release")
    if service.coverage != current.coverage:
        raise ValueError("Service and current-network coverage do not match")
    if optimization.selected_candidate_id != candidate.candidate.candidate_id:
        raise ValueError("Optimization selection does not match the candidate Resource")
    if optimization.selected_scenario_resource_name != sources.candidate_resource_name:
        raise ValueError("Optimization does not reference the candidate Resource")
    if candidate.baseline_coverage != current.coverage:
        raise ValueError("Candidate baseline coverage does not match the current network")
    if candidate.baseline_costs != current.costs:
        raise ValueError("Candidate baseline costs do not match the current network")
    if network_map.baseline_resource_name != sources.current_resource_name:
        raise ValueError("Map baseline does not reference the current-network Resource")
    if network_map.candidate_resource_name != sources.candidate_resource_name:
        raise ValueError("Map candidate does not reference the candidate Resource")
    if network_map.geojson_resource_name != sources.geojson_resource_name:
        raise ValueError("Map does not reference the supplied GeoJSON Resource")

    markdown = _render_markdown(
        inspection=inspection,
        current=current,
        optimization=optimization,
        candidate=candidate,
        network_map=network_map,
        sources=sources,
    )
    return IndonesiaDecisionReport(
        release=release,
        sources=sources,
        markdown=markdown,
        markdown_sha256=hashlib.sha256(markdown.encode("utf-8")).hexdigest(),
        checks=[
            "All report inputs share one exact Dataset Release.",
            "Service coverage equals the complete current-network coverage.",
            "The optimization selection references the supplied candidate scenario.",
            "Candidate baseline coverage and costs equal the current network.",
            "The map manifest references the supplied current, candidate and GeoJSON Resources.",
            "The Markdown body is rendered deterministically from typed Resource fields.",
        ],
    )


def _render_markdown(
    *,
    inspection: IndonesiaDatasetInspection,
    current: IndonesiaCurrentNetworkAnalysis,
    optimization: IndonesiaLocationOptimization,
    candidate: IndonesiaCandidateScenario,
    network_map: IndonesiaNetworkMap,
    sources: IndonesiaDecisionReportSources,
) -> str:
    current_provinces = {
        province.province_code: province for province in current.provinces
    }
    best = [current_provinces[code] for code in current.best_province_codes[:3]]
    priority = [current_provinces[code] for code in current.priority_province_codes[:3]]
    policy = inspection.policy
    distance = policy["distance"]
    time = policy["time"]
    service_clock = policy["service_clock"]
    selected_name = candidate.candidate.candidate_name

    evidence_rows = [
        (
            "indonesia_dataset_inspection.v1",
            sources.inspection_resource_name,
            "Dataset Release、来源、规模、校验和规划参数",
        ),
        (
            "indonesia_service_baseline.v1",
            sources.service_resource_name,
            "现网末端一日、两日、三日服务覆盖",
        ),
        (
            "indonesia_current_network_analysis.v1",
            sources.current_resource_name,
            "现网覆盖、成本、仓库、链路和省份排名",
        ),
        (
            "indonesia_location_optimization.v1",
            sources.optimization_resource_name,
            "完整候选集、目标状态、达标数量、目标函数和入选候选",
        ),
        (
            "indonesia_candidate_scenario.v1",
            sources.candidate_resource_name,
            "入选方案覆盖、成本、差值、容量和省份对比",
        ),
        (
            "indonesia_network_map.v1",
            sources.map_resource_name,
            "地图清单、图层、现网与候选方案引用",
        ),
        (
            "geojson.v1",
            sources.geojson_resource_name,
            "浏览器地图使用的有界地理要素",
        ),
    ]
    evidence_table = "\n".join(
        f"| `{schema}` | `{name}` | {facts} |"
        for schema, name, facts in evidence_rows
    )
    best_table = _province_table(best)
    priority_table = _province_table(priority)
    coverage_rows = "\n".join(
        (
            f"| {_day_label(day)} | {_pct(current.coverage.demand_coverage[day])} | "
            f"{_pct(candidate.candidate_coverage.demand_coverage[day])} | "
            f"{_signed_pct(candidate.demand_coverage_delta[day])} |"
        )
        for day in ("1_day", "2_day", "3_day")
    )
    cost_rows = "\n".join(
        [
            _cost_row(
                "干线运输",
                current.costs.linehaul_idr,
                candidate.candidate_costs.linehaul_idr,
                None,
            ),
            _cost_row(
                "末端运输",
                current.costs.last_mile_idr,
                candidate.candidate_costs.last_mile_idr,
                None,
            ),
            _cost_row(
                "运输总成本",
                current.costs.transport_total_idr,
                candidate.candidate_costs.transport_total_idr,
                candidate.transport_cost_delta_idr,
            ),
            _cost_row(
                "年度固定成本",
                current.costs.annual_fixed_cost_idr,
                candidate.candidate_costs.annual_fixed_cost_idr,
                None,
            ),
            _cost_row(
                f"摊销建设成本（{candidate.opening_amortization_years}年）",
                current.costs.annualized_opening_cost_idr,
                candidate.candidate_costs.annualized_opening_cost_idr,
                None,
            ),
            _cost_row(
                "年度决策总成本",
                current.costs.annual_decision_cost_idr,
                candidate.candidate_costs.annual_decision_cost_idr,
                candidate.annual_decision_cost_delta_idr,
            ),
        ]
    )
    attributions = "<br>".join(inspection.source_attribution)

    return f"""# 印度尼西亚仓库网络决策报告

## 证据索引

| Artifact schema | 原样 Resource 名称 | 负责的事实 |
|---|---|---|
{evidence_table}

## 一、事实

数据集为 `{inspection.data_classification}`，版本 `{inspection.release.version}`；包含 {inspection.customer_count:,} 名客户、{inspection.annual_demand_units:,} 件年需求、{inspection.province_count} 个省份、{inspection.central_warehouse_count} 个中心仓、{inspection.forward_warehouse_count} 个前置仓、{inspection.candidate_location_count} 个已审核候选点和 {inspection.quote_row_count:,} 行运输报价。来源：{attributions}。
证据：`{sources.inspection_resource_name}`。

### 现网覆盖与入选方案

| 指标 | 现网 | {selected_name} | Resource 原样差值 |
|---|---:|---:|---:|
{coverage_rows}

现网证据：`{sources.current_resource_name}`。入选方案与差值证据：`{sources.candidate_resource_name}`。

### 现网省份时效排名

排名合同以两日需求覆盖率为首要指标；最好省份按覆盖率降序、平均服务天数升序排列，需要改善省份按覆盖率升序、平均服务天数降序排列。

最好三个省份：

| 省份代码 | 省份 | 两日需求覆盖率 | 需求加权平均服务天数 |
|---|---|---:|---:|
{best_table}

最需要改善的三个省份：

| 省份代码 | 省份 | 两日需求覆盖率 | 需求加权平均服务天数 |
|---|---|---:|---:|
{priority_table}

证据：`{sources.current_resource_name}`。

### 候选优化

状态为 `{optimization.status}`。完整评估了 {optimization.evaluated_candidate_count} 个已审核候选点，其中 {optimization.target_met_candidate_count} 个达到 {optimization.target_service_days} 日需求覆盖率 {_pct(optimization.target_demand_coverage)} 的目标。入选方案为 `{candidate.candidate.candidate_id}`（{selected_name}）；选择目标是在达标候选中最小化年度决策总成本。
证据：`{sources.optimization_resource_name}`。

### 年度成本

| 成本项 | 现网 IDR/年 | {selected_name} IDR/年 | Resource 原样差值 IDR/年 |
|---|---:|---:|---:|
{cost_rows}

现网证据：`{sources.current_resource_name}`。入选方案与差值证据：`{sources.candidate_resource_name}`。

## 二、假设

| 假设 | 当前合同 |
|---|---|
| 距离 | `{distance["formula"]}`，道路系数 {distance["road_factor"]}，未调用导航 API |
| 平均速度 | {time["average_speed_kph"]} km/h |
| 每日驾驶时间 | {time["driver_hours_per_day"]} 小时 |
| 客户时效口径 | `{service_clock["scope"]}`；干线补货时间不计入客户承诺 |
| 建设费用 | {candidate.opening_amortization_years} 年直线摊销 |
| 优化范围 | 一次只增加一个已审核前置仓候选点 |

证据：`{sources.inspection_resource_name}` 与 `{sources.candidate_resource_name}`。

## 三、分析

- 现网两日需求覆盖率为 {_pct(current.coverage.demand_coverage["2_day"])}；优化目标为 {_pct(optimization.target_demand_coverage)}，Tool 返回状态 `{optimization.status}`。
- `{candidate.candidate.candidate_id}` 方案的两日需求覆盖率为 {_pct(candidate.candidate_coverage.demand_coverage["2_day"])}，Resource 原样差值为 {_signed_pct(candidate.demand_coverage_delta["2_day"])}。
- 该方案的年度运输总成本为 IDR {_idr(candidate.candidate_costs.transport_total_idr)}，Resource 原样差值为 IDR {_signed_idr(candidate.transport_cost_delta_idr)}。
- 该方案的年度决策总成本为 IDR {_idr(candidate.candidate_costs.annual_decision_cost_idr)}，Resource 原样差值为 IDR {_signed_idr(candidate.annual_decision_cost_delta_idr)}。

证据：`{sources.optimization_resource_name}` 与 `{sources.candidate_resource_name}`。

## 四、建议

如果 {_pct(optimization.target_demand_coverage)} 的两日需求覆盖率是必须满足的约束，采用 `{candidate.candidate.candidate_id}`：它是 {optimization.target_met_candidate_count} 个达标候选中年度决策总成本最低的方案。该选择同时意味着接受年度决策总成本的 Resource 原样差值 IDR {_signed_idr(candidate.annual_decision_cost_delta_idr)}。如果不能接受该成本差值，应调整目标或成本约束后重新运行完整候选评估，不应把当前结果解释为无成本的改善。
证据：`{sources.optimization_resource_name}` 与 `{sources.candidate_resource_name}`。

## 五、局限

- 数据分类为 `{inspection.data_classification}`，不是真实商业运营记录。
- 距离采用球面距离乘道路系数，不是导航路网距离。
- 平均速度和每日驾驶小时数是统一规划参数，不表达实时交通、轮渡、天气或司机排班。
- 本次优化只评估单个已审核候选点，不覆盖多仓组合、关仓或中心仓调整。
- 地图仅包含有界省份、仓库和干线要素，不包含客户点。

## 六、缺失证据

- 真实订单、真实运输账单、仓租、人力和建设合同。
- 有缓存、限流和成本控制的导航或路网距离结果。
- 多仓组合、关仓及中心仓调整的可行方案。
- 需求增长、成本变化和服务参数的敏感度分析。

## 对比地图

地图清单：`{sources.map_resource_name}`。GeoJSON：`{sources.geojson_resource_name}`。
地图标题：{network_map.title}。
浏览器可渲染的 `map.v3` 卡片由 Visualization Agent 独立交付，并应紧随本报告显示。
"""


def _province_table(provinces: list) -> str:
    return "\n".join(
        (
            f"| `{province.province_code}` | {province.province_name} | "
            f"{_pct(province.demand_coverage['2_day'])} | "
            f"{_days(province.demand_weighted_average_service_days)} |"
        )
        for province in provinces
    )


def _cost_row(label: str, baseline: int, scenario: int, delta: int | None) -> str:
    rendered_delta = "Resource 未单列" if delta is None else _signed_idr(delta)
    return (
        f"| {label} | {_idr(baseline)} | {_idr(scenario)} | "
        f"{rendered_delta} |"
    )


def _day_label(day: str) -> str:
    return {"1_day": "一日需求覆盖率", "2_day": "两日需求覆盖率", "3_day": "三日需求覆盖率"}[
        day
    ]


def _pct(value: float) -> str:
    return f"{value:.2%}"


def _signed_pct(value: float) -> str:
    return f"{value:+.2%}"


def _idr(value: int) -> str:
    return f"{value:,}"


def _signed_idr(value: int) -> str:
    return f"{value:+,}"


def _days(value: float | None) -> str:
    return "无需求" if value is None else f"{value:.2f}"

from __future__ import annotations

import hashlib
import json
import shutil
from pathlib import Path

import pytest
from supply_chain_planner.indonesia_analysis import (
    build_location_optimization,
    evaluate_candidate_scenario,
    evaluate_current_network,
    evaluate_service_baseline,
    inspect_dataset_release,
    load_network_data,
    prepare_network_map,
    require_valid_indonesia_resource,
    validate_indonesia_resource,
)
from supply_chain_planner.indonesia_models import (
    DatasetReleaseBinding,
    IndonesiaDecisionReportSources,
    IndonesiaGeoJsonRef,
)
from supply_chain_planner.indonesia_report import build_decision_report
from supply_chain_planner.workspace_dataset import load_workspace_dataset_release

ROOT = Path(__file__).resolve().parents[1]
SOURCE_RELEASE = ROOT / "examples" / "indonesia-tutorial" / "releases" / "1.0.0"
WORKSPACE_ID = "0198d5b5-7d0f-7a62-8d9a-f6472dbfab11"
RELEASE_ID = "0198d5b5-7d0f-7a62-8d9a-f6472dbfab12"
DATASET_ID = "indonesia-warehouse-network-tutorial"
VERSION = "1.0.0"
FILE_CONTRACT = {
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


@pytest.fixture(scope="module")
def prepared_release(tmp_path_factory: pytest.TempPathFactory):
    workspace_root = tmp_path_factory.mktemp("workspaces") / WORKSPACE_ID
    files_root = workspace_root / "datasets" / DATASET_ID / VERSION / "files"
    files_root.mkdir(parents=True)
    descriptors = []
    for name, (role, media_type) in sorted(FILE_CONTRACT.items()):
        target = files_root / name
        shutil.copy2(SOURCE_RELEASE / name, target)
        content_sha256 = hashlib.sha256(target.read_bytes()).hexdigest()
        descriptors.append(
            {
                "logicalName": name,
                "role": role,
                "mediaType": media_type,
                "byteSize": target.stat().st_size,
                "contentSha256": content_sha256,
                "relativePath": f"files/{name}",
            }
        )
    display_name = "Indonesia Warehouse Network Tutorial"
    description = "Deterministic synthetic tutorial data."
    digest = hashlib.sha256()
    for value in (
        "workspace.dataset-release.v1",
        DATASET_ID,
        VERSION,
        display_name,
        description,
    ):
        _update_digest(digest, value)
    for descriptor in descriptors:
        for value in (
            descriptor["logicalName"],
            descriptor["role"],
            descriptor["mediaType"],
            str(descriptor["byteSize"]),
            descriptor["contentSha256"],
        ):
            _update_digest(digest, value)
    content_sha256 = digest.hexdigest()
    manifest = {
        "schemaVersion": "workspace.dataset-release.v1",
        "releaseId": RELEASE_ID,
        "datasetId": DATASET_ID,
        "version": VERSION,
        "displayName": display_name,
        "description": description,
        "contentSha256": content_sha256,
        "files": descriptors,
    }
    (files_root.parent / "release.json").write_text(
        json.dumps(manifest, indent=2) + "\n\n",
        encoding="utf-8",
    )
    binding = DatasetReleaseBinding(
        workspace_id=WORKSPACE_ID,
        release_id=RELEASE_ID,
        dataset_id=DATASET_ID,
        version=VERSION,
        content_sha256=content_sha256,
    )
    release = load_workspace_dataset_release(workspace_root, binding)
    inspection = inspect_dataset_release(release)
    data = load_network_data(release, validate_customer_geometry=False)
    return workspace_root, binding, release, inspection, data


def test_inspects_exact_release_without_raw_customer_rows(prepared_release) -> None:
    _, _, _, inspection, _ = prepared_release

    assert inspection.customer_count == 240_000
    assert inspection.annual_demand_units == 6_908_721
    assert inspection.province_count == 38
    assert inspection.central_warehouse_count == 3
    assert inspection.forward_warehouse_count == 8
    assert inspection.candidate_location_count == 20
    assert inspection.quote_row_count == 1_148
    assert "customer_id" not in inspection.model_dump_json()


def test_current_network_reconciles_generator_metrics(prepared_release) -> None:
    *_, data = prepared_release

    service = evaluate_service_baseline(data)
    current = evaluate_current_network(data)

    assert service.coverage == current.coverage
    assert service.provinces == current.provinces
    assert service.province_ranking_policy == current.province_ranking_policy
    assert service.province_ranking_policy.primary_metric == "two_day_demand_coverage"
    assert service.priority_province_codes == current.priority_province_codes
    assert current.priority_province_codes[:3] == ["IDN031", "IDN034", "IDN028"]
    assert current.best_province_codes[:3] == ["IDN022", "IDN015", "IDN011"]
    assert "costs" not in service.model_dump(mode="json")
    assert "warehouses" not in service.model_dump(mode="json")
    assert "links" not in service.model_dump(mode="json")
    assert validate_indonesia_resource(service.model_dump(mode="json")).valid
    assert current.coverage.demand_coverage["1_day"] == pytest.approx(0.30424271004719977)
    assert current.coverage.demand_coverage["2_day"] == pytest.approx(0.7180428910068882)
    assert current.coverage.demand_coverage["3_day"] == pytest.approx(0.9059607415033839)
    assert current.costs.linehaul_idr == 46_795_403_300
    assert current.costs.last_mile_idr == 63_229_294_100
    assert current.costs.transport_total_idr == 110_024_697_400
    assert len(current.provinces) == 38
    provinces = {province.province_name: province for province in current.provinces}
    for name, coverage, average_days in [
        ("Maluku", 0.00, 5.32),
        ("Papua Barat Daya", 0.00, 4.68),
        ("Sulawesi Tenggara", 0.00, 3.58),
        ("Kalimantan Selatan", 1.00, 1.14),
        ("Jawa Timur", 0.9815, 1.73),
        ("DKI Jakarta", 0.9470, 2.05),
    ]:
        assert provinces[name].demand_coverage["2_day"] == pytest.approx(
            coverage,
            abs=0.00005,
        )
        assert provinces[name].demand_weighted_average_service_days == pytest.approx(
            average_days,
            abs=0.005,
        )
    for name in ["Kalimantan Utara", "Papua Barat", "Papua Selatan"]:
        assert provinces[name].demand_units == 0
        assert provinces[name].demand_weighted_average_service_days is None
    assert validate_indonesia_resource(current.model_dump(mode="json")).valid
    invalid = current.model_dump(mode="json")
    invalid["costs"]["transport_total_idr"] += 1
    with pytest.raises(ValueError, match="Current transport cost components do not reconcile"):
        require_valid_indonesia_resource(invalid)
    invalid_ranking = current.model_dump(mode="json")
    invalid_ranking["best_province_codes"] = list(
        reversed(invalid_ranking["best_province_codes"])
    )
    with pytest.raises(ValueError, match="Best province ranking is inconsistent"):
        require_valid_indonesia_resource(invalid_ranking)


def test_candidate_scenario_respects_capacity_and_builds_bounded_map(
    prepared_release,
) -> None:
    *_, data = prepared_release
    current = evaluate_current_network(data)
    scenario = evaluate_candidate_scenario(data, "CAN-PONTIANAK", 5)

    assert scenario.candidate.selected_demand_units <= (scenario.candidate.annual_capacity_units)
    assert (
        scenario.candidate_coverage.demand_coverage["2_day"]
        >= (scenario.baseline_coverage.demand_coverage["2_day"])
    )
    assert scenario.candidate.selected_demand_units == 249_999
    assert scenario.candidate_coverage.demand_coverage["2_day"] == pytest.approx(
        0.741797070687903
    )
    assert scenario.candidate_costs.transport_total_idr == 109_048_262_500
    assert scenario.transport_cost_delta_idr == -976_434_900
    assert scenario.candidate_costs.annual_decision_cost_idr == 125_473_262_500
    assert scenario.annual_decision_cost_delta_idr == 15_448_565_100
    assert validate_indonesia_resource(scenario.model_dump(mode="json")).valid

    prepared_map = prepare_network_map(
        current,
        scenario,
        baseline_resource_name="baseline",
        candidate_resource_name="candidate",
    )
    assert len(prepared_map.geojson["features"]) < 200
    assert not any(
        feature["properties"].get("customer_id") for feature in prepared_map.geojson["features"]
    )
    map_resource = prepared_map.to_resource(
        geojson_resource_name="geojson.v1-test",
        geojson_ref=IndonesiaGeoJsonRef(uri="supply-chain-indonesia://geojson/geojson.v1-test"),
    )
    assert validate_indonesia_resource(map_resource.model_dump(mode="json")).valid


def test_optimization_evaluates_all_candidates_or_declines_unneeded_site(
    prepared_release,
) -> None:
    *_, data = prepared_release

    optimization, scenario, evaluations = build_location_optimization(
        data,
        target_service_days=2,
        target_demand_coverage=0.74,
        opening_amortization_years=5,
    )

    assert optimization.status in {"target_met", "best_available"}
    assert optimization.evaluated_candidate_count == 20
    assert optimization.target_met_candidate_count == 6
    assert len(evaluations) == 20
    assert scenario is not None
    assert scenario.candidate.candidate_id == optimization.selected_candidate_id
    assert optimization.status == "target_met"
    assert optimization.selected_candidate_id == "CAN-JAMBI"
    assert scenario.candidate.selected_demand_units == 250_000
    assert scenario.candidate_coverage.demand_coverage == pytest.approx(
        {
            "1_day": 0.3120233108269968,
            "2_day": 0.7533174374822779,
            "3_day": 0.9222468239779837,
        }
    )
    assert scenario.candidate_costs.transport_total_idr == 107_189_163_600
    assert scenario.transport_cost_delta_idr == -2_835_533_800
    assert scenario.candidate_costs.annual_decision_cost_idr == 123_614_163_600
    assert scenario.annual_decision_cost_delta_idr == 13_589_466_200


def test_decision_report_is_deterministic_and_contains_only_owned_deltas(
    prepared_release,
) -> None:
    *_, inspection, data = prepared_release
    service = evaluate_service_baseline(data)
    current = evaluate_current_network(data)
    optimization, scenario, _ = build_location_optimization(
        data,
        target_service_days=2,
        target_demand_coverage=0.74,
        opening_amortization_years=5,
    )
    assert scenario is not None
    sources = IndonesiaDecisionReportSources(
        inspection_resource_name="indonesia_dataset_inspection.v1-" + "1" * 24,
        service_resource_name="indonesia_service_baseline.v1-" + "2" * 24,
        current_resource_name="indonesia_current_network_analysis.v1-" + "3" * 24,
        optimization_resource_name="indonesia_location_optimization.v1-" + "4" * 24,
        candidate_resource_name="indonesia_candidate_scenario.v1-" + "5" * 24,
        map_resource_name="indonesia_network_map.v1-" + "6" * 24,
        geojson_resource_name="geojson.v1-" + "7" * 24,
    )
    optimization = optimization.model_copy(
        update={"selected_scenario_resource_name": sources.candidate_resource_name}
    )
    prepared_map = prepare_network_map(
        current,
        scenario,
        baseline_resource_name=sources.current_resource_name,
        candidate_resource_name=sources.candidate_resource_name,
    )
    network_map = prepared_map.to_resource(
        geojson_resource_name=sources.geojson_resource_name,
        geojson_ref=IndonesiaGeoJsonRef(
            uri=(
                "supply-chain-indonesia://geojson/"
                f"{sources.geojson_resource_name}"
            )
        ),
    )

    report = build_decision_report(
        inspection=inspection,
        service=service,
        current=current,
        optimization=optimization,
        candidate=scenario,
        network_map=network_map,
        sources=sources,
    )

    assert validate_indonesia_resource(report.model_dump(mode="json")).valid
    assert "57.5%" not in report.markdown
    assert "42.5%" not in report.markdown
    assert "1.33" not in report.markdown
    assert "卸下" not in report.markdown
    assert "Resource 未单列" in report.markdown
    assert "::codex-inline-vis{" not in report.markdown
    assert "Visualization Agent 独立交付" in report.markdown
    assert validate_indonesia_resource(optimization.model_dump(mode="json")).valid

    invalid_count = optimization.model_dump(mode="json")
    invalid_count["target_met_candidate_count"] = 8
    with pytest.raises(ValueError, match="target-met count is inconsistent"):
        require_valid_indonesia_resource(invalid_count)

    no_site, no_scenario, no_evaluations = build_location_optimization(
        data,
        target_service_days=2,
        target_demand_coverage=0.70,
        opening_amortization_years=5,
    )
    assert no_site.status == "target_already_met"
    assert no_site.selected_candidate_id is None
    assert no_site.target_met_candidate_count == 0
    assert no_scenario is None
    assert no_evaluations == []


def _update_digest(digest: hashlib._Hash, value: str) -> None:
    encoded = value.encode("utf-8")
    digest.update(len(encoded).to_bytes(8, "big"))
    digest.update(encoded)

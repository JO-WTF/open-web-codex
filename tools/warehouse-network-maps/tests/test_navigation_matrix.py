from __future__ import annotations

import asyncio
import json
import tempfile
from pathlib import Path
from types import SimpleNamespace

import pytest
from maps_mcp import server


def _context(workspace) -> SimpleNamespace:
    meta = SimpleNamespace(
        model_extra={"codex/sandbox-state-meta": {"sandboxCwd": workspace.as_uri()}}
    )
    return SimpleNamespace(request_context=SimpleNamespace(meta=meta))


def _request() -> dict[str, object]:
    return {
        "schema_version": "navigation_matrix_request.v2",
        "input_identity": {
            "schema_version": "prepared_network_input.v2",
            "content_sha256": "0" * 64,
        },
        "warehouse_scope": {"kind": "all_warehouses"},
        "warehouse_ids": ["CENTER-1"],
        "estimated_billable_elements": 2,
        "routes": [
            {
                "origin_id": "CENTER-1",
                "destination_id": "CITY-1",
                "layer": "last_mile",
                "origin_longitude": 106.8,
                "origin_latitude": -6.2,
                "destination_longitude": 106.9,
                "destination_latitude": -6.3,
            },
            {
                "origin_id": "CENTER-1",
                "destination_id": "CITY-2",
                "layer": "last_mile",
                "origin_longitude": 106.8,
                "origin_latitude": -6.2,
                "destination_longitude": 107.0,
                "destination_latitude": -6.4,
            },
        ],
    }


def _selected_request() -> dict[str, object]:
    request = _request()
    request["warehouse_scope"] = {
        "kind": "selected_warehouses",
        "warehouse_ids": [" CENTER-1 "],
    }
    return request


def test_execute_navigation_matrix_writes_typed_workspace_facts(tmp_path, monkeypatch) -> None:
    output_dir = tmp_path / "outputs/warehouse-network/requests"
    output_dir.mkdir(parents=True)
    request_path = output_dir / "navigation-request.json"
    result_path = output_dir / "navigation-result.json"
    request_path.write_text(json.dumps(_request()), encoding="utf-8")

    class FakeClient:
        async def get_route(self, *_args, **_kwargs):
            raise AssertionError("multi-route origin group must use distance_matrix")

        async def distance_matrix(self, origins, destinations, *, mode):
            assert origins == [{"longitude": 106.8, "latitude": -6.2}]
            assert len(destinations) == 2
            assert mode == "driving"
            return {
                "provider": "mapbox",
                "entries": [
                    {
                        "originIndex": 0,
                        "destinationIndex": 0,
                        "distanceMeters": 1_200,
                        "durationSeconds": 300,
                    },
                    {
                        "originIndex": 0,
                        "destinationIndex": 1,
                        "distanceMeters": None,
                        "durationSeconds": None,
                    },
                ],
            }

    async def fake_client(_ctx):
        return FakeClient()

    monkeypatch.setattr(server, "_client", fake_client)
    result = asyncio.run(
        server.execute_navigation_matrix(
            "outputs/warehouse-network/requests/navigation-request.json",
            "outputs/warehouse-network/requests/navigation-result.json",
            _context(tmp_path),
        )
    )
    assert result.provider == "mapbox"
    assert result.ready_pair_count == 1
    assert result.unreachable_pair_count == 1
    payload = json.loads(result_path.read_text())
    assert payload["schema_version"] == "navigation_matrix_result.v2"
    assert payload["warehouse_scope"] == {"kind": "all_warehouses"}
    assert payload["warehouse_ids"] == ["CENTER-1"]
    assert payload["rows"][0]["distance_km"] == 1.2
    assert payload["rows"][1]["status"] == "unreachable"

    with pytest.raises(ValueError, match="generated_output_path_invalid"):
        asyncio.run(
            server.execute_navigation_matrix(
                "outputs/warehouse-network/requests/navigation-request.json",
                "navigation-result.json",
                _context(tmp_path),
            )
        )


def test_navigation_contract_rejects_noncanonical_warehouse_ids() -> None:
    with pytest.raises(ValueError, match="warehouse_ids_not_canonical"):
        server.NavigationMatrixRequest.model_validate(
            {**_request(), "warehouse_ids": ["CENTER-1", "CENTER-1"]}
        )


def test_navigation_rows_preserve_zero_duration_and_classify_missing_or_malformed_values() -> None:
    route = server.NavigationRouteRequest.model_validate(_request()["routes"][0])

    zero = server._matrix_row(
        route,
        {"distanceMeters": 0, "durationSeconds": 0},
        provider="mapbox",
        mode="driving",
    )
    assert zero.status == "ready"
    assert zero.distance_km == 0
    assert zero.duration_hours == 0

    unreachable = server._matrix_row(
        route,
        {"distanceMeters": None, "durationSeconds": None},
        provider="mapbox",
        mode="driving",
    )
    assert unreachable.status == "unreachable"

    malformed = server._matrix_row(
        route,
        {"distanceMeters": "zero", "durationSeconds": "not-a-duration"},
        provider="mapbox",
        mode="driving",
    )
    assert malformed.status == "error"


def test_single_navigation_route_uses_route_provider_and_preserves_zero_values(
    tmp_path, monkeypatch
) -> None:
    output_dir = tmp_path / "outputs/warehouse-network/requests"
    output_dir.mkdir(parents=True)
    request = _request()
    request["routes"] = [request["routes"][0]]
    request["estimated_billable_elements"] = 1
    (output_dir / "single-route-request.json").write_text(json.dumps(request), encoding="utf-8")
    calls: list[str] = []

    class FakeClient:
        async def get_route(self, origin, destination, *, mode):
            calls.append("get_route")
            assert origin == {"longitude": 106.8, "latitude": -6.2}
            assert destination == {"longitude": 106.9, "latitude": -6.3}
            assert mode == "driving"
            return {
                "provider": "mapbox",
                "code": "Ok",
                "route_count": 1,
                "routes": [{"distance": 0, "duration": 0}],
            }

        async def distance_matrix(self, *_args, **_kwargs):
            raise AssertionError("single-route origin group must not use distance_matrix")

    async def fake_client(_ctx):
        return FakeClient()

    monkeypatch.setattr(server, "_client", fake_client)
    result = asyncio.run(
        server.execute_navigation_matrix(
            "outputs/warehouse-network/requests/single-route-request.json",
            "outputs/warehouse-network/requests/single-route-result.json",
            _context(tmp_path),
        )
    )
    payload = json.loads((output_dir / "single-route-result.json").read_text(encoding="utf-8"))
    assert calls == ["get_route"]
    assert result.ready_pair_count == 1
    assert payload["rows"][0]["status"] == "ready"
    assert payload["rows"][0]["distance_km"] == 0
    assert payload["rows"][0]["duration_hours"] == 0


def test_single_navigation_route_classifies_no_route_and_rejects_malformed_results() -> None:
    route = server.NavigationRouteRequest.model_validate(_request()["routes"][0])

    provider, no_route = server._single_navigation_entry(
        {"provider": "mapbox", "code": "NoRoute", "route_count": 0, "routes": []}
    )
    no_route_row = server._matrix_row(route, no_route, provider=provider, mode="driving")
    assert no_route_row.status == "unreachable"

    google_provider, google_entry = server._single_navigation_entry(
        {
            "provider": "google",
            "route_count": 1,
            "routes": [{"distanceMeters": 0, "duration": "0s"}],
        }
    )
    assert server._matrix_row(
        route,
        google_entry,
        provider=google_provider,
        mode="driving",
    ).status == "ready"

    with pytest.raises(RuntimeError, match="navigation_route_result_invalid"):
        server._single_navigation_entry(
            {"provider": "mapbox", "code": "Ok", "route_count": 1, "routes": []}
        )


def test_navigation_contract_round_trips_selected_warehouse_scope(tmp_path, monkeypatch) -> None:
    output_dir = tmp_path / "outputs/warehouse-network/requests"
    output_dir.mkdir(parents=True)
    request_path = output_dir / "selected-request.json"
    result_path = output_dir / "selected-result.json"
    request_path.write_text(json.dumps(_selected_request()), encoding="utf-8")

    class FakeClient:
        async def distance_matrix(self, _origins, destinations, *, mode):
            assert mode == "driving"
            return {
                "provider": "mapbox",
                "entries": [
                    {
                        "originIndex": 0,
                        "destinationIndex": index,
                        "distanceMeters": 1_000 + index,
                        "durationSeconds": 300,
                    }
                    for index, _destination in enumerate(destinations)
                ],
            }

    async def fake_client(_ctx):
        return FakeClient()

    monkeypatch.setattr(server, "_client", fake_client)
    execution = asyncio.run(
        server.execute_navigation_matrix(
            "outputs/warehouse-network/requests/selected-request.json",
            "outputs/warehouse-network/requests/selected-result.json",
            _context(tmp_path),
        )
    )
    assert execution.navigation_matrix_relative_path == (
        "outputs/warehouse-network/requests/selected-result.json"
    )
    result = server.NavigationMatrixResult.model_validate(
        json.loads(result_path.read_text(encoding="utf-8"))
    )
    assert result.warehouse_scope.model_dump() == {
        "kind": "selected_warehouses",
        "warehouse_ids": ["CENTER-1"],
    }
    assert result.warehouse_ids == ["CENTER-1"]
    assert len(result.rows) == 2

    mismatched = _selected_request()
    mismatched["warehouse_ids"] = ["OTHER"]
    with pytest.raises(ValueError, match="warehouse_scope_selected_warehouse_ids_mismatch"):
        server.NavigationMatrixRequest.model_validate(mismatched)


def test_publish_workspace_geojson_publishes_validated_polygon_boundaries(tmp_path) -> None:
    (tmp_path / "boundaries.geojson").write_text(
        json.dumps(
            {
                "type": "FeatureCollection",
                "features": [
                    {
                        "type": "Feature",
                        "properties": {"kind": "administrative_boundary"},
                        "geometry": {
                            "type": "Polygon",
                            "coordinates": [
                                [[106.0, -7.0], [107.0, -7.0], [107.0, -6.0], [106.0, -7.0]]
                            ],
                        },
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    with tempfile.TemporaryDirectory() as directory:
        original = server._resource_store
        server._resource_store = server.GeoJsonResourceStore(Path(directory))
        try:
            result = asyncio.run(
                server.publish_workspace_geojson(
                    "boundaries.geojson",
                    _context(tmp_path),
                    require_polygon=True,
                )
            )
            assert result.structuredContent is not None
            assert result.structuredContent["data_ref"]["profile"]["feature_types"][0][
                "geometry_types"
            ] == ["Polygon"]
        finally:
            server._resource_store = original

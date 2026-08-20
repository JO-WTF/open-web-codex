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
            "schema_version": "prepared_network_input.v1",
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


def test_execute_navigation_matrix_writes_typed_workspace_facts(tmp_path, monkeypatch) -> None:
    output_dir = tmp_path / "outputs/warehouse-network/requests"
    output_dir.mkdir(parents=True)
    request_path = output_dir / "navigation-request.json"
    result_path = output_dir / "navigation-result.json"
    request_path.write_text(json.dumps(_request()), encoding="utf-8")

    class FakeClient:
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

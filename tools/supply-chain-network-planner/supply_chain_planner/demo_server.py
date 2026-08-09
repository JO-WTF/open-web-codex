"""Explicit, deterministic synthetic source generation for an authorized Workspace."""

from __future__ import annotations

import argparse
import asyncio
import csv
import hashlib
import io
import json
import tempfile
from collections.abc import Iterable
from pathlib import Path
from typing import Any, Literal

from mcp.server.fastmcp import Context, FastMCP
from mcp.server.stdio import stdio_server

from .geo import haversine_km, stable_uniform
from .workspace_intake import (
    discover,
    workspace_contains_supported_sources,
)
from .workspace_scope import trusted_workspace_root

SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"
TEMPLATE_ID = "indonesia-network-tutorial"
TEMPLATE_VERSION = "1.0.0"
TEMPLATE_REF = f"{TEMPLATE_ID}@{TEMPLATE_VERSION}"
LARGE_CITY_DEMAND_COUNT = 50
DEFAULT_SEED = 42
MANIFEST_NAME = "demo-manifest.json"
TEMPLATES = {TEMPLATE_REF: (TEMPLATE_ID, TEMPLATE_VERSION)}
TUTORIAL_FIXTURE_ROOT = (
    Path(__file__).resolve().parents[1] / "examples" / "indonesia-network" / "base"
)
TUTORIAL_FIXTURE_FILES = (
    "demand-cities.csv",
    "existing-warehouses.csv",
    "route-quotes.csv",
    "administrative-areas.json",
    "candidate-warehouses.csv",
    "source-lock.json",
    "validation-report.json",
    "dataset-manifest.json",
)

mcp = FastMCP(
    "Supply Chain Demo Data",
    instructions=(
        "Generate synthetic Workspace source files only after the user explicitly requests "
        "Demo, sample, or synthetic data. Resolve the destination exclusively from trusted "
        "Turn Workspace metadata. Never accept a path or Workspace identifier, never overwrite "
        "existing data, and never create a Dataset Release or start analysis."
    ),
    json_response=True,
)


def _csv_bytes(fieldnames: list[str], rows: Iterable[dict[str, object]]) -> bytes:
    output = io.StringIO(newline="")
    writer = csv.DictWriter(output, fieldnames=fieldnames, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return output.getvalue().encode("utf-8")


def _large_cities() -> list[dict[str, object]]:
    locations = (
        ("Jakarta", -6.2088, 106.8456),
        ("Bandung", -6.9175, 107.6191),
        ("Semarang", -6.9667, 110.4167),
        ("Surabaya", -7.2575, 112.7521),
        ("Medan", 3.5952, 98.6722),
        ("Pekanbaru", 0.5071, 101.4478),
        ("Padang", -0.9471, 100.4172),
        ("Palembang", -2.9761, 104.7754),
        ("Banjarmasin", -3.3186, 114.5944),
        ("Balikpapan", -1.2379, 116.8529),
        ("Samarinda", -0.5022, 117.1536),
        ("Pontianak", -0.0263, 109.3425),
        ("Denpasar", -8.6500, 115.2167),
        ("Mataram", -8.5833, 116.1167),
        ("Kupang", -10.1772, 123.6070),
        ("Bandar Lampung", -5.4292, 105.2610),
        ("Makassar", -5.1477, 119.4327),
        ("Manado", 1.4748, 124.8421),
        ("Palu", -0.9003, 119.8780),
        ("Kendari", -3.9985, 122.5120),
        ("Ambon", -3.6954, 128.1814),
        ("Jayapura", -2.5916, 140.6690),
        ("Sorong", -0.8762, 131.2558),
        ("Ternate", 0.7907, 127.3842),
    )
    return [
        {
            "city_id": f"city-{index + 1:02d}",
            "name": name,
            "region": f"region-{index // 4 + 1:02d}",
            "latitude": latitude,
            "longitude": longitude,
        }
        for index, (name, latitude, longitude) in enumerate(locations)
    ]


def _large_facilities(cities: list[dict[str, object]]) -> list[dict[str, object]]:
    definitions = (
        ("ID1", "ID1-Jakarta", 0, "existing", 3_500_000, 420_000, 0, 0.95),
        ("ID2", "ID2-Medan", 4, "existing", 3_500_000, 440_000, 0, 1.00),
        (
            "ID3",
            "ID3-Banjarmasin",
            8,
            "candidate",
            2_400_000,
            280_000,
            1_200_000,
            0.82,
        ),
        (
            "ID4",
            "ID4-Denpasar",
            12,
            "candidate",
            2_400_000,
            290_000,
            1_250_000,
            0.84,
        ),
        (
            "ID5",
            "ID5-Makassar",
            16,
            "candidate",
            2_400_000,
            285_000,
            1_180_000,
            0.83,
        ),
        (
            "ID6",
            "ID6-Ambon",
            20,
            "candidate",
            2_400_000,
            295_000,
            1_300_000,
            0.86,
        ),
    )
    return [
        {
            "facility_id": facility_id,
            "name": name,
            "city_id": cities[city_index]["city_id"],
            "existing_or_candidate": status,
            "capacity": capacity,
            "fixed_cost": fixed_cost,
            "opening_cost": opening_cost,
            "handling_cost": handling_cost,
        }
        for (
            facility_id,
            name,
            city_index,
            status,
            capacity,
            fixed_cost,
            opening_cost,
            handling_cost,
        ) in definitions
    ]


def _large_distance_rate(seed: int, origin_city_id: str, destination_city_id: str) -> str:
    key = f"{seed}:rate:{origin_city_id}:{destination_city_id}"
    return f"{stable_uniform(key, 0.010, 0.016):.6f}"


def _large_template_files(seed: int) -> dict[str, bytes]:
    cities = _large_cities()
    city_by_id = {str(city["city_id"]): city for city in cities}
    facilities = _large_facilities(cities)
    existing = [
        facility for facility in facilities if facility["existing_or_candidate"] == "existing"
    ]

    origin_city_ids = sorted({str(facility["city_id"]) for facility in facilities})
    destination_city_ids = [str(city["city_id"]) for city in cities]
    city_demands = [
        {
            "city_id": str(city["city_id"]),
            "date": "2026-01-15",
            "quantity": 100_000 * (index + 1)
            + int(stable_uniform(f"{seed}:city-demand:{city['city_id']}", 0, 50_000)),
        }
        for index, city in enumerate(cities)
    ]
    coverage = []
    for city in cities:
        city_point = (float(city["longitude"]), float(city["latitude"]))
        facility = min(
            existing,
            key=lambda item: haversine_km(
                city_point,
                (
                    float(city_by_id[str(item["city_id"])]["longitude"]),
                    float(city_by_id[str(item["city_id"])]["latitude"]),
                ),
            ),
        )
        coverage.append(
            {
                "facility_id": facility["facility_id"],
                "city_id": city["city_id"],
                "is_current": "true",
            }
        )
    lanes = []
    for origin_city_id in origin_city_ids:
        origin = city_by_id[origin_city_id]
        for destination_city_id in destination_city_ids:
            destination = city_by_id[destination_city_id]
            distance_km = (
                haversine_km(
                    (float(origin["longitude"]), float(origin["latitude"])),
                    (float(destination["longitude"]), float(destination["latitude"])),
                )
                * 1.2
            )
            lanes.append(
                {
                    "origin_city_id": origin_city_id,
                    "destination_city_id": destination_city_id,
                    "distance_km": f"{distance_km:.3f}",
                    "travel_time_hours": f"{distance_km / 55:.3f}",
                    "base_cost_per_unit": "0.75",
                    "distance_cost_per_km_per_unit": _large_distance_rate(
                        seed, origin_city_id, destination_city_id
                    ),
                    "currency": "IDR",
                }
            )
    parameters = {
        "schemaVersion": "warehouse_network_demo_parameters.v1",
        "dataClassification": "synthetic_demo",
        "market": "ID",
        "planning_mode": "candidate_warehouse_optimization",
        "cost_scope": "fixed, opening, handling, and transport costs",
        "target_sla_hours": 48,
        "coverage_target": 90,
    }
    return {
        "cities.csv": _csv_bytes(list(cities[0]), cities),
        "city-demand.csv": _csv_bytes(["city_id", "date", "quantity"], city_demands),
        "facilities.csv": _csv_bytes(list(facilities[0]), facilities),
        "warehouse-city-coverage.csv": _csv_bytes(
            ["facility_id", "city_id", "is_current"], coverage
        ),
        "city-lanes.csv": _csv_bytes(list(lanes[0]), lanes),
        "planning-parameters.json": (
            json.dumps(parameters, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n"
        ).encode("utf-8"),
    }


def _template_files(template_ref: str, seed: int) -> dict[str, bytes]:
    if template_ref == TEMPLATE_REF:
        if not TUTORIAL_FIXTURE_ROOT.is_dir():
            raise ValueError("demo_template_unavailable")
        return {
            name: (TUTORIAL_FIXTURE_ROOT / name).read_bytes() for name in TUTORIAL_FIXTURE_FILES
        }
    raise ValueError("demo_template_unavailable")


def _template_statistics(template_ref: str) -> dict[str, int]:
    if template_ref == TEMPLATE_REF:
        return {
            "cityCount": 50,
            "cityDemandCount": LARGE_CITY_DEMAND_COUNT,
            "facilityCount": 11,
            "routeCount": 580,
            "lastMileRouteCount": 550,
            "linehaulRouteCount": 30,
        }
    raise ValueError("demo_template_unavailable")


def _sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _manifest_bytes(template_ref: str, seed: int, files: dict[str, bytes]) -> bytes:
    template_id, version = TEMPLATES[template_ref]
    payload = {
        "schemaVersion": "demo_workspace_sources.v1",
        "dataClassification": "synthetic_demo",
        "template": {"id": template_id, "version": version, "seed": seed},
        "statistics": _template_statistics(template_ref),
        "files": [
            {"displayName": name, "bytes": len(content), "contentSha256": _sha256(content)}
            for name, content in sorted(files.items())
        ],
    }
    return (
        json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n"
    ).encode("utf-8")


def _target_relative(template_ref: str) -> Path:
    template_id, version = TEMPLATES[template_ref]
    return Path("demo-data") / f"{template_id}-{version}"


def _assert_safe_workspace(root: Path, target_relative: Path) -> None:
    if root.is_symlink() or not root.is_dir():
        raise ValueError("trusted_workspace_is_not_a_directory")
    target = root / target_relative
    current = root
    for part in target_relative.parts:
        current = current / part
        if current.exists() and current.is_symlink():
            raise ValueError("demo_target_symlink_rejected")
    if target.exists() and not target.is_dir():
        raise ValueError("demo_target_conflict")


def _verify_existing(target: Path, template_ref: str, seed: int, files: dict[str, bytes]) -> bool:
    manifest = _manifest_bytes(template_ref, seed, files)
    expected = {**files, MANIFEST_NAME: manifest}
    if not target.is_dir() or target.is_symlink():
        return False
    actual_names = sorted(
        path.name for path in target.iterdir() if path.is_file() and not path.is_symlink()
    )
    if actual_names != sorted(expected):
        return False
    return all((target / name).read_bytes() == content for name, content in expected.items())


def _source_result(
    root: Path,
    status: Literal["created", "reused"],
    template_ref: str,
    seed: int,
    files: dict[str, bytes],
) -> dict[str, Any]:
    template_id, version = TEMPLATES[template_ref]
    manifest = _manifest_bytes(template_ref, seed, files)
    names = set(files) | {MANIFEST_NAME}
    sources = [item for item in discover(root) if Path(item["relative_path"]).name in names]
    if len(sources) != len(names):
        raise ValueError("demo_source_discovery_incomplete")
    return {
        "schemaVersion": "demo_workspace_sources.v1",
        "status": status,
        "dataClassification": "synthetic_demo",
        "template": {"id": template_id, "version": version, "seed": seed},
        "contentSummary": {
            "sourceFileCount": len(files),
            "totalBytes": sum(len(content) for content in files.values()),
            "manifestSha256": _sha256(manifest),
            **_template_statistics(template_ref),
        },
        "sources": sources,
    }


def create_sources(root: Path, template_id: str, seed: int) -> dict[str, Any]:
    if template_id not in TEMPLATES:
        raise ValueError("demo_template_unavailable")
    if seed < 0 or seed > 2_147_483_647:
        raise ValueError("demo_seed_out_of_range")
    target_relative = _target_relative(template_id)
    _assert_safe_workspace(root, target_relative)
    target = root / target_relative
    files = _template_files(template_id, seed)
    if target.exists():
        if _verify_existing(target, template_id, seed, files):
            return _source_result(root, "reused", template_id, seed, files)
        raise ValueError("demo_target_conflict")
    if workspace_contains_supported_sources(root):
        raise ValueError("workspace_contains_supported_sources")

    target.parent.mkdir(parents=True, exist_ok=True)
    staging_root = root / ".demo-staging"
    staging_root.mkdir(exist_ok=True)
    template_name, _ = TEMPLATES[template_id]
    temporary = Path(tempfile.mkdtemp(prefix=f"{template_name}-", dir=staging_root))
    try:
        for name, content in files.items():
            (temporary / name).write_bytes(content)
        (temporary / MANIFEST_NAME).write_bytes(_manifest_bytes(template_id, seed, files))
        try:
            temporary.rename(target)
        except OSError:
            if _verify_existing(target, template_id, seed, files):
                return _source_result(root, "reused", template_id, seed, files)
            raise ValueError("demo_target_conflict") from None
    finally:
        if temporary.exists():
            for child in temporary.iterdir():
                child.unlink()
            temporary.rmdir()
        try:
            staging_root.rmdir()
        except OSError:
            pass
    return _source_result(root, "created", template_id, seed, files)


@mcp.tool(structured_output=True)
def create_demo_workspace_sources(
    ctx: Context,
    template_id: str = TEMPLATE_REF,
    seed: int = DEFAULT_SEED,
) -> dict[str, Any]:
    """Generate the reviewed large synthetic source set in the trusted empty Workspace."""
    return create_sources(trusted_workspace_root(ctx.request_context.meta), template_id, seed)


async def run_stdio() -> None:
    initialization_options = mcp._mcp_server.create_initialization_options(
        experimental_capabilities={SANDBOX_STATE_META_CAPABILITY: {}},
    )
    async with stdio_server() as streams:
        await mcp._mcp_server.run(streams[0], streams[1], initialization_options)


def main() -> None:
    parser = argparse.ArgumentParser(description="Supply-chain Demo Workspace source MCP server")
    parser.add_argument(
        "--workspace-root",
        type=Path,
        default=Path.cwd(),
        help="Plugin root; never used as the target Workspace",
    )
    parser.add_argument("--transport", choices=("stdio",), default="stdio")
    parser.parse_args()
    asyncio.run(run_stdio())


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Generate the reproducible Indonesia warehouse-network tutorial dataset."""

from __future__ import annotations

import argparse
import csv
import gzip
import hashlib
import io
import json
import math
import random
import re
import shutil
import sys
import urllib.request
from collections import defaultdict
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Any

TOOL_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOL_ROOT))

from supply_chain_planner.network.geo import (  # noqa: E402
    Coordinate,
    ProvinceBoundary,
    haversine_km,
    pearson_correlation,
    spearman_correlation,
    stable_uniform,
)

EXAMPLE_ROOT = TOOL_ROOT / "examples" / "indonesia-tutorial"
DEFAULT_RELEASE_ROOT = EXAMPLE_ROOT / "releases" / "1.0.0"
NETWORK_EXAMPLE_ROOT = TOOL_ROOT / "examples" / "indonesia-network"
NETWORK_DATASET_ID = "indonesia-network-tutorial"
NETWORK_DATASET_VERSION = "1.0.0"

# The city snapshot is intentionally small and human-auditable. Values are
# rounded BPS city-population figures used only to create tutorial demand;
# the boundary snapshot remains the authoritative geometry check.
TOP_50_CITIES = (
    ("JAKARTA", "Jakarta", "Jakarta", 10_562_088, -6.2088, 106.8456),
    ("SURABAYA", "Surabaya", "East Java", 2_874_314, -7.2575, 112.7521),
    ("BEKASI", "Bekasi", "West Java", 2_543_676, -6.2383, 106.9756),
    ("BANDUNG", "Bandung", "West Java", 2_510_103, -6.9175, 107.6191),
    ("MEDAN", "Medan", "North Sumatra", 2_435_252, 3.5952, 98.6722),
    ("DEPOK", "Depok", "West Java", 2_056_335, -6.4025, 106.7942),
    ("TANGERANG", "Tangerang", "Banten", 1_930_556, -6.1783, 106.6319),
    ("PALEMBANG", "Palembang", "South Sumatra", 1_668_848, -2.9761, 104.7754),
    ("SEMARANG", "Semarang", "Central Java", 1_653_524, -6.9667, 110.4167),
    ("MAKASSAR", "Makassar", "South Sulawesi", 1_423_877, -5.1477, 119.4327),
    ("SOUTH_TANGERANG", "Tangerang Selatan", "Banten", 1_354_350, -6.2886, 106.7179),
    ("BATAM", "Batam", "Riau Islands", 1_196_396, 1.1301, 104.0529),
    ("BANDAR_LAMPUNG", "Bandar Lampung", "Lampung", 1_166_066, -5.4292, 105.2619),
    ("BOGOR", "Bogor", "West Java", 1_043_070, -6.5950, 106.8167),
    ("PEKANBARU", "Pekanbaru", "Riau", 983_356, 0.5071, 101.4478),
    ("PADANG", "Padang", "West Sumatra", 909_040, -0.9471, 100.4172),
    ("MALANG", "Malang", "East Java", 843_810, -7.9666, 112.6326),
    ("SAMARINDA", "Samarinda", "East Kalimantan", 827_994, -0.5022, 117.1536),
    ("DENPASAR", "Denpasar", "Bali", 725_314, -8.6500, 115.2167),
    ("TASIKMALAYA", "Tasikmalaya", "West Java", 733_467, -7.3274, 108.2207),
    ("BALIKPAPAN", "Balikpapan", "East Kalimantan", 704_110, -1.2379, 116.8529),
    ("SERANG", "Serang", "Banten", 692_101, -6.1201, 106.1503),
    ("PONTIANAK", "Pontianak", "West Kalimantan", 658_685, -0.0263, 109.3425),
    ("BANJARMASIN", "Banjarmasin", "South Kalimantan", 657_663, -3.3186, 114.5944),
    ("JAMBI", "Jambi", "Jambi", 604_736, -1.6101, 103.6131),
    ("MANADO", "Manado", "North Sulawesi", 451_916, 1.4748, 124.8421),
    ("KUPANG", "Kupang", "East Nusa Tenggara", 442_758, -10.1772, 123.6070),
    ("MATARAM", "Mataram", "West Nusa Tenggara", 441_561, -8.5833, 116.1167),
    ("CILEGON", "Cilegon", "Banten", 434_896, -6.0027, 106.0119),
    ("YOGYAKARTA", "Yogyakarta", "Special Region of Yogyakarta", 414_704, -7.7956, 110.3695),
    ("JAYAPURA", "Jayapura", "Papua", 398_478, -2.5916, 140.6690),
    ("BENGKULU", "Bengkulu", "Bengkulu", 373_591, -3.7928, 102.2608),
    ("PALU", "Palu", "Central Sulawesi", 373_218, -0.9003, 119.8780),
    ("SUKABUMI", "Sukabumi", "West Java", 353_838, -6.9277, 106.9299),
    ("AMBON", "Ambon", "Maluku", 347_484, -3.6954, 128.1814),
    ("KENDARI", "Kendari", "Southeast Sulawesi", 345_107, -3.9985, 122.5120),
    ("CIREBON", "Cirebon", "West Java", 341_235, -6.7320, 108.5523),
    ("DUMAI", "Dumai", "Riau", 323_452, 1.6671, 101.4432),
    ("BINJAI", "Binjai", "North Sumatra", 291_842, 3.6001, 98.4850),
    ("KEDIRI", "Kediri", "East Java", 289_418, -7.8480, 112.0178),
    ("SORONG", "Sorong", "Southwest Papua", 284_410, -0.8762, 131.2558),
    ("TEGAL", "Tegal", "Central Java", 273_825, -6.8694, 109.1402),
    ("PEMATANGSIANTAR", "Pematang Siantar", "North Sumatra", 268_254, 2.9595, 99.0687),
    ("BANDA_ACEH", "Banda Aceh", "Aceh", 252_899, 5.5483, 95.3238),
    ("TARAKAN", "Tarakan", "North Kalimantan", 242_786, 3.3000, 117.6333),
    ("PROBOLINGGO", "Probolinggo", "East Java", 239_125, -7.7543, 113.2159),
    ("SINGKAWANG", "Singkawang", "West Kalimantan", 235_064, 0.9060, 108.9872),
    ("BATU", "Batu", "East Java", 213_046, -7.8671, 112.5239),
    ("PASURUAN", "Pasuruan", "East Java", 208_006, -7.6453, 112.9075),
    ("LHOKSEUMAWE", "Lhokseumawe", "Aceh", 188_713, 5.1801, 97.1507),
)


@dataclass(frozen=True)
class Site:
    site_id: str
    name: str
    site_type: str
    province_code: str
    latitude: float
    longitude: float
    current_parent_center_id: str | None = None

    @property
    def point(self) -> Coordinate:
        return self.longitude, self.latitude


CENTRAL_WAREHOUSES = (
    Site("CEN-BEKASI", "Bekasi Central Warehouse", "central", "IDN012", -6.2416, 106.9924),
    Site(
        "CEN-SIDOARJO",
        "Sidoarjo Central Warehouse",
        "central",
        "IDN015",
        -7.4478,
        112.7183,
    ),
    Site(
        "CEN-MAKASSAR",
        "Makassar Central Warehouse",
        "central",
        "IDN027",
        -5.1477,
        119.4327,
    ),
)

FORWARD_WAREHOUSES = (
    Site(
        "FWD-MEDAN",
        "Medan Forward Warehouse",
        "forward",
        "IDN002",
        3.5897,
        98.6738,
        "CEN-BEKASI",
    ),
    Site(
        "FWD-PALEMBANG",
        "Palembang Forward Warehouse",
        "forward",
        "IDN006",
        -2.9909,
        104.7566,
        "CEN-BEKASI",
    ),
    Site(
        "FWD-SEMARANG",
        "Semarang Forward Warehouse",
        "forward",
        "IDN013",
        -6.9667,
        110.4167,
        "CEN-BEKASI",
    ),
    Site(
        "FWD-DENPASAR",
        "Denpasar Forward Warehouse",
        "forward",
        "IDN017",
        -8.6705,
        115.2126,
        "CEN-SIDOARJO",
    ),
    Site(
        "FWD-BANJARMASIN",
        "Banjarmasin Forward Warehouse",
        "forward",
        "IDN022",
        -3.3186,
        114.5944,
        "CEN-SIDOARJO",
    ),
    Site(
        "FWD-BALIKPAPAN",
        "Balikpapan Forward Warehouse",
        "forward",
        "IDN023",
        -1.2379,
        116.8529,
        "CEN-SIDOARJO",
    ),
    Site(
        "FWD-MANADO",
        "Manado Forward Warehouse",
        "forward",
        "IDN025",
        1.4748,
        124.8421,
        "CEN-MAKASSAR",
    ),
    Site(
        "FWD-JAYAPURA",
        "Jayapura Forward Warehouse",
        "forward",
        "IDN033",
        -2.5916,
        140.669,
        "CEN-MAKASSAR",
    ),
)

CANDIDATE_LOCATIONS = (
    Site("CAN-BANDA-ACEH", "Banda Aceh Candidate", "candidate", "IDN001", 5.5483, 95.3238),
    Site("CAN-PADANG", "Padang Candidate", "candidate", "IDN003", -0.9471, 100.4172),
    Site("CAN-PEKANBARU", "Pekanbaru Candidate", "candidate", "IDN004", 0.5071, 101.4478),
    Site("CAN-JAMBI", "Jambi Candidate", "candidate", "IDN005", -1.6101, 103.6131),
    Site("CAN-BENGKULU", "Bengkulu Candidate", "candidate", "IDN007", -3.7928, 102.2608),
    Site(
        "CAN-BANDAR-LAMPUNG",
        "Bandar Lampung Candidate",
        "candidate",
        "IDN008",
        -5.3971,
        105.2668,
    ),
    Site("CAN-PONTIANAK", "Pontianak Candidate", "candidate", "IDN020", -0.0263, 109.3425),
    Site(
        "CAN-PALANGKARAYA",
        "Palangkaraya Candidate",
        "candidate",
        "IDN021",
        -2.2161,
        113.9137,
    ),
    Site("CAN-SAMARINDA", "Samarinda Candidate", "candidate", "IDN023", -0.5022, 117.1536),
    Site("CAN-TARAKAN", "Tarakan Candidate", "candidate", "IDN024", 3.3, 117.6333),
    Site("CAN-MATARAM", "Mataram Candidate", "candidate", "IDN018", -8.5833, 116.1167),
    Site("CAN-KUPANG", "Kupang Candidate", "candidate", "IDN019", -10.1772, 123.607),
    Site("CAN-PALU", "Palu Candidate", "candidate", "IDN026", -0.9003, 119.878),
    Site("CAN-KENDARI", "Kendari Candidate", "candidate", "IDN028", -3.9985, 122.512),
    Site("CAN-AMBON", "Ambon Candidate", "candidate", "IDN031", -3.6954, 128.1814),
    Site("CAN-TERNATE", "Ternate Candidate", "candidate", "IDN032", 0.7907, 127.3842),
    Site("CAN-SORONG", "Sorong Candidate", "candidate", "IDN034", -0.8762, 131.2558),
    Site("CAN-MANOKWARI", "Manokwari Candidate", "candidate", "IDN035", -0.8615, 134.062),
    Site("CAN-NABIRE", "Nabire Candidate", "candidate", "IDN037", -3.4, 135.5),
    Site("CAN-MERAUKE", "Merauke Candidate", "candidate", "IDN036", -8.4932, 140.4018),
)

LEGACY_BOUNDARY_NAMES = {
    "Aceh": "Aceh",
    "Bali": "Bali",
    "Banten": "Banten",
    "Bengkulu": "Bengkulu",
    "Daerah Istimewa Yogyakarta": "Special Region of Yogyakarta",
    "DKI Jakarta": "Jakarta Special Capital Region",
    "Gorontalo": "Gorontalo",
    "Jambi": "Jambi",
    "Jawa Barat": "West Java",
    "Jawa Tengah": "Central Java",
    "Jawa Timur": "East Java",
    "Kalimantan Barat": "West Kalimantan",
    "Kalimantan Selatan": "South Kalimantan",
    "Kalimantan Tengah": "Central Kalimantan",
    "Kalimantan Timur": "East Kalimantan",
    "Kalimantan Utara": "North Kalimantan",
    "Kepulauan Bangka Belitung": "Bangka-Belitung Islands",
    "Kepulauan Riau": "Riau Islands",
    "Lampung": "Lampung",
    "Maluku": "Maluku",
    "Maluku Utara": "North Maluku",
    "Nusa Tenggara Barat": "West Nusa Tenggara",
    "Nusa Tenggara Timur": "East Nusa Tenggara",
    "Papua": "Papua",
    "Papua Barat": "West Papua",
    "Papua Barat Daya": "West Papua",
    "Papua Pegunungan": "Papua",
    "Papua Selatan": "Papua",
    "Papua Tengah": "Papua",
    "Riau": "Riau",
    "Sulawesi Barat": "West Sulawesi",
    "Sulawesi Selatan": "South Sulawesi",
    "Sulawesi Tengah": "Central Sulawesi",
    "Sulawesi Tenggara": "Southeast Sulawesi",
    "Sulawesi Utara": "North Sulawesi",
    "Sumatera Barat": "West Sumatra",
    "Sumatera Selatan": "South Sumatra",
    "Sumatera Utara": "North Sumatra",
}

LEGACY_CONTRACT_FORWARD = {
    "IDN001": "FWD-MEDAN",
    "IDN002": "FWD-MEDAN",
    "IDN003": "FWD-PALEMBANG",
    "IDN004": "FWD-PALEMBANG",
    "IDN005": "FWD-PALEMBANG",
    "IDN006": "FWD-PALEMBANG",
    "IDN007": "FWD-PALEMBANG",
    "IDN008": "FWD-PALEMBANG",
    "IDN009": "FWD-PALEMBANG",
    "IDN010": "FWD-PALEMBANG",
    "IDN011": "FWD-SEMARANG",
    "IDN012": "FWD-SEMARANG",
    "IDN013": "FWD-SEMARANG",
    "IDN014": "FWD-SEMARANG",
    "IDN015": "FWD-SEMARANG",
    "IDN016": "FWD-SEMARANG",
    "IDN017": "FWD-DENPASAR",
    "IDN018": "FWD-DENPASAR",
    "IDN019": "FWD-DENPASAR",
    "IDN020": "FWD-BANJARMASIN",
    "IDN021": "FWD-BANJARMASIN",
    "IDN022": "FWD-BANJARMASIN",
    "IDN023": "FWD-BANJARMASIN",
    "IDN024": "FWD-BALIKPAPAN",
    "IDN025": "FWD-MANADO",
    "IDN026": "FWD-MANADO",
    "IDN027": "FWD-MANADO",
    "IDN028": "FWD-MANADO",
    "IDN029": "FWD-MANADO",
    "IDN030": "FWD-MANADO",
    "IDN031": "FWD-MANADO",
    "IDN032": "FWD-MANADO",
    "IDN033": "FWD-JAYAPURA",
    "IDN034": "FWD-JAYAPURA",
    "IDN035": "FWD-JAYAPURA",
    "IDN036": "FWD-JAYAPURA",
    "IDN037": "FWD-JAYAPURA",
    "IDN038": "FWD-JAYAPURA",
}

FORWARD_CAPACITY_BUFFERS = {
    "FWD-MEDAN": 1.12,
    "FWD-PALEMBANG": 1.1,
    "FWD-SEMARANG": 1.05,
    "FWD-DENPASAR": 1.08,
    "FWD-BANJARMASIN": 1.15,
    "FWD-BALIKPAPAN": 1.08,
    "FWD-MANADO": 1.12,
    "FWD-JAYAPURA": 1.18,
}


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _json_load(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def _json_write(path: Path, value: Any) -> None:
    path.write_text(
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


def _download(url: str, destination: Path) -> None:
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "open-web-codex-indonesia-tutorial/1.0"},
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        destination.write_bytes(response.read())


def _round_capacity(value: int, factor: float) -> int:
    return max(10_000, math.ceil(value * factor / 10_000) * 10_000)


def _round_money(value: float, increment: int = 100) -> int:
    return int(round(value / increment) * increment)


def _macro_region(province_code: str) -> str:
    number = int(province_code.removeprefix("IDN"))
    if number <= 10:
        return "sumatra"
    if number <= 16:
        return "java"
    if number == 17:
        return "bali"
    if number <= 19:
        return "nusa_tenggara"
    if number <= 24:
        return "kalimantan"
    if number <= 30:
        return "sulawesi"
    if number <= 32:
        return "maluku"
    return "papua"


def _lane_factor(origin_region: str, destination_region: str) -> float:
    if origin_region == destination_region:
        return 1.0
    factor = 1.16
    if destination_region in {"maluku", "papua"}:
        factor += 0.16
    elif destination_region == "nusa_tenggara":
        factor += 0.08
    return factor


def _service_days(distance_km: float, speed_kph: float, driver_hours: float) -> int:
    return max(1, math.ceil(distance_km / speed_kph / driver_hours))


@contextmanager
def _gzip_csv_writer(
    path: Path,
    fieldnames: list[str],
) -> Iterator[csv.DictWriter]:
    with path.open("wb") as raw:
        with gzip.GzipFile(
            filename="",
            mode="wb",
            fileobj=raw,
            compresslevel=9,
            mtime=0,
        ) as compressed:
            with io.TextIOWrapper(compressed, encoding="utf-8", newline="") as text:
                writer = csv.DictWriter(text, fieldnames=fieldnames, lineterminator="\n")
                writer.writeheader()
                yield writer


def _write_csv(path: Path, fieldnames: list[str], rows: list[dict[str, Any]]) -> None:
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def _load_population(path: Path) -> list[dict[str, Any]]:
    with path.open(encoding="utf-8", newline="") as handle:
        rows = [
            {
                "province_code": row["province_code"],
                "province_name": row["province_name"],
                "population_2025": int(row["population_2025"]),
            }
            for row in csv.DictReader(handle)
        ]
    if len(rows) != 38:
        raise ValueError(f"population source has {len(rows)} rows, expected 38")
    if sum(row["population_2025"] for row in rows) != 284_438_600:
        raise ValueError("rounded province population rows do not sum to the expected 284,438,600")
    return rows


def _load_boundaries(
    current_path: Path,
    legacy_path: Path,
) -> tuple[
    dict[str, ProvinceBoundary],
    dict[str, ProvinceBoundary],
    dict[str, Any],
]:
    current_geojson = _json_load(current_path)
    legacy_geojson = _json_load(legacy_path)
    current = {
        boundary.code: boundary
        for feature in current_geojson["features"]
        for boundary in [
            ProvinceBoundary.from_feature(
                feature,
                code_field="ADM1CD_c",
                name_field="NAM_1",
            )
        ]
    }
    legacy = {
        boundary.name: boundary
        for feature in legacy_geojson["features"]
        for boundary in [
            ProvinceBoundary.from_feature(
                feature,
                code_field="shapeID",
                name_field="shapeName",
            )
        ]
    }
    if len(current) != 38:
        raise ValueError(f"current boundary has {len(current)} provinces, expected 38")
    if len(legacy) != 34:
        raise ValueError(f"legacy boundary has {len(legacy)} provinces, expected 34")
    return current, legacy, current_geojson


def _allocate_customer_counts(
    population_rows: list[dict[str, Any]],
    policy: dict[str, Any],
    customer_count: int,
) -> dict[str, int]:
    exponent = float(policy["population_weight_exponent"])
    factors = policy["commercial_factors"]
    disabled = policy["zero_demand_provinces"]
    weighted: list[tuple[str, float]] = []
    for row in population_rows:
        code = row["province_code"]
        weight = 0.0
        if code not in disabled:
            weight = row["population_2025"] ** exponent * float(factors.get(code, 1.0))
        weighted.append((code, weight))
    total_weight = sum(weight for _, weight in weighted)
    exact = {
        code: customer_count * weight / total_weight if weight else 0.0 for code, weight in weighted
    }
    allocated = {code: math.floor(value) for code, value in exact.items()}
    remainder = customer_count - sum(allocated.values())
    order = sorted(
        exact,
        key=lambda code: (exact[code] - allocated[code], code),
        reverse=True,
    )
    for code in order[:remainder]:
        allocated[code] += 1
    if sum(allocated.values()) != customer_count:
        raise AssertionError("customer allocation does not preserve the requested total")
    return allocated


def _joint_sample(
    current: ProvinceBoundary,
    legacy: ProvinceBoundary,
    rng: random.Random,
) -> Coordinate:
    for _ in range(20_000):
        point = current.sample(rng)
        if legacy.contains(point):
            return point
    raise RuntimeError(f"failed to find an overlapping point for current province {current.name}")


def _customer_point(
    current: ProvinceBoundary,
    legacy: ProvinceBoundary,
    clusters: list[Coordinate],
    clustered_share: float,
    rng: random.Random,
) -> Coordinate:
    if rng.random() < clustered_share:
        min_lon, min_lat, max_lon, max_lat = current.bounds
        longitude_scale = min(0.35, max(0.02, (max_lon - min_lon) * 0.04))
        latitude_scale = min(0.25, max(0.02, (max_lat - min_lat) * 0.04))
        anchor = clusters[rng.randrange(len(clusters))]
        for _ in range(40):
            candidate = (
                round(rng.gauss(anchor[0], longitude_scale), 6),
                round(rng.gauss(anchor[1], latitude_scale), 6),
            )
            if current.contains(candidate) and legacy.contains(candidate):
                return candidate
    for _ in range(100):
        candidate = _joint_sample(current, legacy, rng)
        rounded = round(candidate[0], 6), round(candidate[1], 6)
        if current.contains(rounded) and legacy.contains(rounded):
            return rounded
    raise RuntimeError(f"failed to generate a persisted point for {current.name}")


def _validate_sites(
    sites: tuple[Site, ...],
    current: dict[str, ProvinceBoundary],
    legacy: dict[str, ProvinceBoundary],
) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    for site in sites:
        province = current[site.province_code]
        legacy_name = LEGACY_BOUNDARY_NAMES[province.name]
        current_pass = province.contains(site.point)
        legacy_pass = legacy[legacy_name].contains(site.point)
        results.append(
            {
                "site_id": site.site_id,
                "province_code": site.province_code,
                "province_name": province.name,
                "current_boundary_pass": current_pass,
                "legacy_boundary_pass": legacy_pass,
            }
        )
        if not current_pass or not legacy_pass:
            raise ValueError(
                f"site {site.site_id} is outside its declared province boundary: "
                f"current={current_pass}, legacy={legacy_pass}"
            )
    return results


def _refresh_government_check(
    source: dict[str, Any],
    expected_names: set[str],
    scratch_root: Path,
) -> dict[str, Any]:
    target = scratch_root / "indonesia-government-provinces-38.geojson"
    _download(source["download_url"], target)
    digest = _sha256(target)
    if digest != source["observed_sha256"]:
        raise ValueError(
            "Indonesia government validation geometry changed; review the new "
            f"source before updating the lock (observed {digest})"
        )
    payload = _json_load(target)
    names = {feature["properties"]["wadmpr"] for feature in payload["features"]}
    return {
        "mode": "refreshed",
        "sha256": digest,
        "feature_count": len(payload["features"]),
        "province_names_match": names == expected_names,
    }


def _prepare_sources(
    example_root: Path,
    *,
    download_missing: bool,
) -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    lock = _json_load(example_root / "source-lock.json")
    sources = {source["source_id"]: source for source in lock["sources"]}
    for source in sources.values():
        bundled_path = source.get("bundled_path")
        expected_digest = source.get("sha256")
        if not bundled_path or not expected_digest:
            continue
        path = example_root / bundled_path
        if not path.exists():
            if not download_missing:
                raise FileNotFoundError(
                    f"missing locked source {path}; rerun with --download-missing-sources"
                )
            path.parent.mkdir(parents=True, exist_ok=True)
            _download(source["download_url"], path)
        actual_digest = _sha256(path)
        if actual_digest != expected_digest:
            raise ValueError(
                f"locked source {source['source_id']} digest mismatch: {actual_digest}"
            )
    return lock, sources


def _normalized_boundary_geojson(
    source_geojson: dict[str, Any],
) -> dict[str, Any]:
    features = []
    for feature in source_geojson["features"]:
        properties = feature["properties"]
        features.append(
            {
                "type": "Feature",
                "properties": {
                    "province_code": properties["ADM1CD_c"],
                    "province_name": properties["NAM_1"],
                    "source": "World Bank Official Boundaries",
                },
                "geometry": feature["geometry"],
            }
        )
    features.sort(key=lambda feature: feature["properties"]["province_code"])
    return {"type": "FeatureCollection", "features": features}


def _province_centroid(
    province_code: str,
    demand_units: dict[str, int],
    weighted_longitude: dict[str, float],
    weighted_latitude: dict[str, float],
    clusters: dict[str, list[Coordinate]],
) -> Coordinate:
    units = demand_units[province_code]
    if units:
        return weighted_longitude[province_code] / units, weighted_latitude[province_code] / units
    return clusters[province_code][0]


def _quote_row(
    *,
    quote_id: str,
    leg_type: str,
    origin: Site,
    destination_type: str,
    destination_id: str,
    destination_name: str,
    destination_province_code: str,
    destination_point: Coordinate,
    road_factor: float,
    current_lane: bool,
) -> dict[str, Any]:
    haversine = haversine_km(origin.point, destination_point)
    distance = haversine * road_factor
    origin_region = _macro_region(origin.province_code)
    destination_region = _macro_region(destination_province_code)
    lane_factor = _lane_factor(origin_region, destination_region)
    if leg_type == "linehaul":
        base_cost = 900
        distance_rate = 7.0
        noise_low, noise_high = 0.96, 1.06
    else:
        base_cost = 2200
        distance_rate = 14.0
        noise_low, noise_high = 0.94, 1.08
    noise = stable_uniform(quote_id, noise_low, noise_high)
    quote = _round_money((base_cost + distance_rate * distance) * lane_factor * noise)
    return {
        "quote_id": quote_id,
        "leg_type": leg_type,
        "origin_warehouse_id": origin.site_id,
        "destination_type": destination_type,
        "destination_id": destination_id,
        "destination_name": destination_name,
        "haversine_km": f"{haversine:.3f}",
        "road_factor": f"{road_factor:.3f}",
        "estimated_distance_km": f"{distance:.3f}",
        "base_idr_per_demand_unit": base_cost,
        "distance_rate_idr_per_km_per_demand_unit": f"{distance_rate:.2f}",
        "lane_factor": f"{lane_factor:.3f}",
        "noise_factor": f"{noise:.4f}",
        "quoted_idr_per_demand_unit": quote,
        "is_current_lane": str(current_lane).lower(),
    }


def generate(
    *,
    example_root: Path,
    release_root: Path,
    customer_count: int | None,
    replace: bool,
    download_missing_sources: bool,
    refresh_external_validation: bool,
) -> dict[str, Any]:
    lock, sources = _prepare_sources(
        example_root,
        download_missing=download_missing_sources,
    )
    policy = _json_load(example_root / "generation-policy.json")
    requested_customer_count = customer_count or int(policy["customer_count"])
    if requested_customer_count <= 0:
        raise ValueError("customer_count must be positive")
    if release_root.exists():
        if not replace:
            raise FileExistsError(
                f"release path already exists: {release_root}; use --replace explicitly"
            )
        shutil.rmtree(release_root)
    release_root.mkdir(parents=True)
    scratch_root = release_root / ".generation"
    scratch_root.mkdir()

    population_rows = _load_population(example_root / "population-2025.csv")
    population_by_code = {row["province_code"]: row["population_2025"] for row in population_rows}
    name_by_code = {row["province_code"]: row["province_name"] for row in population_rows}
    current_path = example_root / sources["world-bank-official-boundaries-idn-adm1"]["bundled_path"]
    legacy_path = example_root / sources["geoboundaries-gbopen-idn-adm1"]["bundled_path"]
    current, legacy, current_geojson = _load_boundaries(current_path, legacy_path)
    if set(current) != set(population_by_code):
        raise ValueError("current boundary and population province codes differ")
    if {boundary.name for boundary in current.values()} != set(name_by_code.values()):
        raise ValueError("current boundary and population province names differ")
    missing_legacy_names = set(LEGACY_BOUNDARY_NAMES.values()) - set(legacy)
    if missing_legacy_names:
        raise ValueError(f"legacy boundaries are missing names: {missing_legacy_names}")

    government_source = sources["indonesia-ministry-agriculture-provinces-38"]
    if refresh_external_validation:
        government_check = _refresh_government_check(
            government_source,
            set(name_by_code.values()),
            scratch_root,
        )
    else:
        government_check = {
            "mode": "locked_observation",
            "sha256": government_source["observed_sha256"],
            "feature_count": government_source["expected_feature_count"],
            "province_names_match": True,
        }
    if not government_check["province_names_match"]:
        raise ValueError("government and World Bank province name sets differ")

    all_sites = CENTRAL_WAREHOUSES + FORWARD_WAREHOUSES + CANDIDATE_LOCATIONS
    site_checks = _validate_sites(all_sites, current, legacy)
    customer_counts = _allocate_customer_counts(
        population_rows,
        policy,
        requested_customer_count,
    )
    rng = random.Random(int(policy["seed"]))
    cluster_count = int(policy["cluster_count_per_province"])
    clusters: dict[str, list[Coordinate]] = {}
    for row in population_rows:
        code = row["province_code"]
        province = current[code]
        legacy_boundary = legacy[LEGACY_BOUNDARY_NAMES[province.name]]
        clusters[code] = [
            _joint_sample(province, legacy_boundary, rng) for _ in range(cluster_count)
        ]

    boundary_path = release_root / "province-boundaries.geojson"
    _json_write(boundary_path, _normalized_boundary_geojson(current_geojson))

    customer_fields = [
        "customer_id",
        "province_code",
        "province_name",
        "latitude",
        "longitude",
        "annual_demand_units",
        "customer_tier",
    ]
    assignment_fields = [
        "customer_id",
        "current_forward_id",
        "assignment_reason",
        "estimated_distance_km",
        "service_days",
        "nearest_forward_id",
        "nearest_distance_km",
    ]
    clustered_share = float(policy["clustered_customer_share"])
    road_factor = float(policy["road_factor"])
    speed_kph = float(policy["average_speed_kph"])
    driver_hours = float(policy["driver_hours_per_day"])
    assignment_mix = policy["current_assignment_mix"]
    nearest_threshold = float(assignment_mix["nearest_forward"])
    second_threshold = nearest_threshold + float(assignment_mix["second_nearest_legacy"])

    demand_by_province: dict[str, int] = defaultdict(int)
    weighted_longitude: dict[str, float] = defaultdict(float)
    weighted_latitude: dict[str, float] = defaultdict(float)
    assignment_demand: dict[tuple[str, str], int] = defaultdict(int)
    forward_load: dict[str, int] = defaultdict(int)
    coverage_units = {1: 0, 2: 0, 3: 0}
    coverage_customers = {1: 0, 2: 0, 3: 0}
    total_demand_units = 0
    nearest_assignment_count = 0
    current_boundary_passes = 0
    legacy_boundary_passes = 0
    customer_sequence = 0

    customer_path = release_root / "customers.csv.gz"
    assignment_path = release_root / "customer-assignments.csv.gz"
    with (
        _gzip_csv_writer(customer_path, customer_fields) as customer_writer,
        _gzip_csv_writer(assignment_path, assignment_fields) as assignment_writer,
    ):
        for row in population_rows:
            code = row["province_code"]
            province = current[code]
            legacy_boundary = legacy[LEGACY_BOUNDARY_NAMES[province.name]]
            for _ in range(customer_counts[code]):
                customer_sequence += 1
                point = _customer_point(
                    province,
                    legacy_boundary,
                    clusters[code],
                    clustered_share,
                    rng,
                )
                if province.contains(point):
                    current_boundary_passes += 1
                if legacy_boundary.contains(point):
                    legacy_boundary_passes += 1
                distances = sorted(
                    (
                        haversine_km(site.point, point) * road_factor,
                        site.site_id,
                    )
                    for site in FORWARD_WAREHOUSES
                )
                nearest_distance, nearest_id = distances[0]
                second_distance, second_id = distances[1]
                assignment_roll = rng.random()
                if assignment_roll < nearest_threshold:
                    assigned_distance, assigned_id = nearest_distance, nearest_id
                    assignment_reason = "nearest_current_forward"
                elif assignment_roll < second_threshold:
                    assigned_distance, assigned_id = second_distance, second_id
                    assignment_reason = "legacy_second_nearest"
                else:
                    assigned_id = LEGACY_CONTRACT_FORWARD[code]
                    if assigned_id == nearest_id:
                        assigned_id = second_id
                    assigned_distance = next(
                        distance for distance, site_id in distances if site_id == assigned_id
                    )
                    assignment_reason = "legacy_province_contract"
                if assigned_id == nearest_id:
                    nearest_assignment_count += 1

                demand_units = max(1, min(500, round(rng.lognormvariate(3.0, 0.85))))
                if demand_units >= 100:
                    tier = "strategic"
                elif demand_units >= 35:
                    tier = "growth"
                else:
                    tier = "standard"
                service_days = _service_days(
                    assigned_distance,
                    speed_kph,
                    driver_hours,
                )
                customer_id = f"CUS-{customer_sequence:09d}"
                customer_writer.writerow(
                    {
                        "customer_id": customer_id,
                        "province_code": code,
                        "province_name": province.name,
                        "latitude": f"{point[1]:.6f}",
                        "longitude": f"{point[0]:.6f}",
                        "annual_demand_units": demand_units,
                        "customer_tier": tier,
                    }
                )
                assignment_writer.writerow(
                    {
                        "customer_id": customer_id,
                        "current_forward_id": assigned_id,
                        "assignment_reason": assignment_reason,
                        "estimated_distance_km": f"{assigned_distance:.3f}",
                        "service_days": service_days,
                        "nearest_forward_id": nearest_id,
                        "nearest_distance_km": f"{nearest_distance:.3f}",
                    }
                )
                demand_by_province[code] += demand_units
                weighted_longitude[code] += point[0] * demand_units
                weighted_latitude[code] += point[1] * demand_units
                assignment_demand[(assigned_id, code)] += demand_units
                forward_load[assigned_id] += demand_units
                total_demand_units += demand_units
                for day in (1, 2, 3):
                    if service_days <= day:
                        coverage_units[day] += demand_units
                        coverage_customers[day] += 1

    if customer_sequence != requested_customer_count:
        raise AssertionError("generated customer row count differs from requested count")

    province_centroids = {
        code: _province_centroid(
            code,
            demand_by_province,
            weighted_longitude,
            weighted_latitude,
            clusters,
        )
        for code in current
    }
    quote_rows: list[dict[str, Any]] = []
    for center in CENTRAL_WAREHOUSES:
        for forward in FORWARD_WAREHOUSES:
            quote_rows.append(
                _quote_row(
                    quote_id=f"LH-{center.site_id}-{forward.site_id}",
                    leg_type="linehaul",
                    origin=center,
                    destination_type="forward_warehouse",
                    destination_id=forward.site_id,
                    destination_name=forward.name,
                    destination_province_code=forward.province_code,
                    destination_point=forward.point,
                    road_factor=road_factor,
                    current_lane=forward.current_parent_center_id == center.site_id,
                )
            )
        for candidate in CANDIDATE_LOCATIONS:
            quote_rows.append(
                _quote_row(
                    quote_id=f"LH-{center.site_id}-{candidate.site_id}",
                    leg_type="linehaul",
                    origin=center,
                    destination_type="candidate_warehouse",
                    destination_id=candidate.site_id,
                    destination_name=candidate.name,
                    destination_province_code=candidate.province_code,
                    destination_point=candidate.point,
                    road_factor=road_factor,
                    current_lane=False,
                )
            )
    for forward in FORWARD_WAREHOUSES:
        for row in population_rows:
            code = row["province_code"]
            quote_rows.append(
                _quote_row(
                    quote_id=f"LM-{forward.site_id}-{code}",
                    leg_type="last_mile",
                    origin=forward,
                    destination_type="province",
                    destination_id=code,
                    destination_name=row["province_name"],
                    destination_province_code=code,
                    destination_point=province_centroids[code],
                    road_factor=road_factor,
                    current_lane=assignment_demand[(forward.site_id, code)] > 0,
                )
            )
    for candidate in CANDIDATE_LOCATIONS:
        for row in population_rows:
            code = row["province_code"]
            quote_rows.append(
                _quote_row(
                    quote_id=f"LM-{candidate.site_id}-{code}",
                    leg_type="last_mile",
                    origin=candidate,
                    destination_type="province",
                    destination_id=code,
                    destination_name=row["province_name"],
                    destination_province_code=code,
                    destination_point=province_centroids[code],
                    road_factor=road_factor,
                    current_lane=False,
                )
            )
    quote_fields = [
        "quote_id",
        "leg_type",
        "origin_warehouse_id",
        "destination_type",
        "destination_id",
        "destination_name",
        "haversine_km",
        "road_factor",
        "estimated_distance_km",
        "base_idr_per_demand_unit",
        "distance_rate_idr_per_km_per_demand_unit",
        "lane_factor",
        "noise_factor",
        "quoted_idr_per_demand_unit",
        "is_current_lane",
    ]
    quote_path = release_root / "transport-quotes.csv"
    _write_csv(quote_path, quote_fields, quote_rows)
    quote_by_id = {row["quote_id"]: row for row in quote_rows}

    warehouse_rows: list[dict[str, Any]] = []
    center_load: dict[str, int] = defaultdict(int)
    for forward in FORWARD_WAREHOUSES:
        assert forward.current_parent_center_id is not None
        center_load[forward.current_parent_center_id] += forward_load[forward.site_id]
    for site in CENTRAL_WAREHOUSES + FORWARD_WAREHOUSES:
        load = (
            center_load[site.site_id] if site.site_type == "central" else forward_load[site.site_id]
        )
        factor = 1.15 if site.site_type == "central" else FORWARD_CAPACITY_BUFFERS[site.site_id]
        capacity = _round_capacity(load, factor)
        warehouse_rows.append(
            {
                "warehouse_id": site.site_id,
                "warehouse_name": site.name,
                "warehouse_type": site.site_type,
                "province_code": site.province_code,
                "province_name": name_by_code[site.province_code],
                "latitude": f"{site.latitude:.6f}",
                "longitude": f"{site.longitude:.6f}",
                "current_parent_center_id": site.current_parent_center_id or "",
                "annual_capacity_units": capacity,
                "current_annual_demand_units": load,
                "utilization_ratio": f"{load / capacity:.6f}" if capacity else "0.000000",
            }
        )
    warehouse_fields = [
        "warehouse_id",
        "warehouse_name",
        "warehouse_type",
        "province_code",
        "province_name",
        "latitude",
        "longitude",
        "current_parent_center_id",
        "annual_capacity_units",
        "current_annual_demand_units",
        "utilization_ratio",
    ]
    warehouse_path = release_root / "warehouses.csv"
    _write_csv(warehouse_path, warehouse_fields, warehouse_rows)

    link_rows = []
    nearest_parent_count = 0
    for forward in FORWARD_WAREHOUSES:
        parent_id = forward.current_parent_center_id
        assert parent_id is not None
        center_distances = sorted(
            (haversine_km(center.point, forward.point) * road_factor, center.site_id)
            for center in CENTRAL_WAREHOUSES
        )
        nearest_center_id = center_distances[0][1]
        if parent_id == nearest_center_id:
            nearest_parent_count += 1
        quote_id = f"LH-{parent_id}-{forward.site_id}"
        link_rows.append(
            {
                "forward_warehouse_id": forward.site_id,
                "current_center_warehouse_id": parent_id,
                "linehaul_quote_id": quote_id,
                "estimated_distance_km": quote_by_id[quote_id]["estimated_distance_km"],
                "replenishment_days": _service_days(
                    float(quote_by_id[quote_id]["estimated_distance_km"]),
                    speed_kph,
                    driver_hours,
                ),
                "nearest_center_warehouse_id": nearest_center_id,
            }
        )
    link_fields = [
        "forward_warehouse_id",
        "current_center_warehouse_id",
        "linehaul_quote_id",
        "estimated_distance_km",
        "replenishment_days",
        "nearest_center_warehouse_id",
    ]
    link_path = release_root / "warehouse-links.csv"
    _write_csv(link_path, link_fields, link_rows)

    candidate_rows = []
    for site in CANDIDATE_LOCATIONS:
        province_demand = demand_by_province[site.province_code]
        capacity = max(250_000, _round_capacity(int(province_demand * 0.75), 1.0))
        opening_cost = _round_money(42_000_000_000 + capacity * 18_000, 1_000_000)
        annual_fixed_cost = _round_money(
            6_500_000_000 + capacity * 2_500,
            1_000_000,
        )
        nearest_center = min(
            CENTRAL_WAREHOUSES,
            key=lambda center: haversine_km(center.point, site.point),
        )
        candidate_rows.append(
            {
                "candidate_id": site.site_id,
                "candidate_name": site.name,
                "province_code": site.province_code,
                "province_name": name_by_code[site.province_code],
                "latitude": f"{site.latitude:.6f}",
                "longitude": f"{site.longitude:.6f}",
                "annual_capacity_units": capacity,
                "opening_cost_idr": opening_cost,
                "annual_fixed_cost_idr": annual_fixed_cost,
                "proposed_parent_center_id": nearest_center.site_id,
            }
        )
    candidate_fields = [
        "candidate_id",
        "candidate_name",
        "province_code",
        "province_name",
        "latitude",
        "longitude",
        "annual_capacity_units",
        "opening_cost_idr",
        "annual_fixed_cost_idr",
        "proposed_parent_center_id",
    ]
    candidate_path = release_root / "candidate-locations.csv"
    _write_csv(candidate_path, candidate_fields, candidate_rows)

    planning_policy = {
        "schema_version": "indonesia_network_planning_policy.v1",
        "dataset_id": policy["dataset_id"],
        "release_version": policy["release_version"],
        "currency": policy["currency"],
        "distance": {
            "method": policy["distance_method"],
            "road_factor": road_factor,
            "formula": "haversine_km * road_factor",
            "navigation_api_used": False,
        },
        "time": {
            "average_speed_kph": speed_kph,
            "driver_hours_per_day": driver_hours,
            "formula": "max(1, ceil(estimated_distance_km / average_speed_kph / driver_hours_per_day))",
        },
        "service_clock": {
            "scope": policy["customer_service_clock"],
            "linehaul_time_in_customer_promise": False,
            "reason": "Forward inventory is assumed to be stocked before a customer order.",
        },
        "cost_scope": [
            "central_warehouse_to_forward_warehouse_linehaul",
            "forward_warehouse_to_customer_last_mile",
        ],
        "data_classification": "synthetic_tutorial_data",
    }
    policy_path = release_root / "planning-policy.json"
    _json_write(policy_path, planning_policy)

    population_values = [float(population_by_code[row["province_code"]]) for row in population_rows]
    customer_values = [float(customer_counts[row["province_code"]]) for row in population_rows]
    population_customer_spearman = spearman_correlation(
        population_values,
        customer_values,
    )
    correlations: dict[str, dict[str, float]] = {}
    for leg_type in ("linehaul", "last_mile"):
        rows = [row for row in quote_rows if row["leg_type"] == leg_type]
        distances = [float(row["estimated_distance_km"]) for row in rows]
        quotes = [float(row["quoted_idr_per_demand_unit"]) for row in rows]
        correlations[leg_type] = {
            "pearson": pearson_correlation(distances, quotes),
            "spearman": spearman_correlation(distances, quotes),
        }

    linehaul_cost = 0
    for forward in FORWARD_WAREHOUSES:
        parent_id = forward.current_parent_center_id
        assert parent_id is not None
        quote = quote_by_id[f"LH-{parent_id}-{forward.site_id}"]
        linehaul_cost += forward_load[forward.site_id] * int(quote["quoted_idr_per_demand_unit"])
    last_mile_cost = 0
    for (forward_id, province_code), units in assignment_demand.items():
        quote = quote_by_id[f"LM-{forward_id}-{province_code}"]
        last_mile_cost += units * int(quote["quoted_idr_per_demand_unit"])

    gates = policy["quality_gates"]
    nearest_share = nearest_assignment_count / requested_customer_count
    current_boundary_rate = current_boundary_passes / requested_customer_count
    legacy_boundary_rate = legacy_boundary_passes / requested_customer_count
    checks = {
        "current_province_count_is_38": len(current) == 38,
        "legacy_province_count_is_34": len(legacy) == 34,
        "government_province_names_match": government_check["province_names_match"],
        "rounded_province_population_sum_matches_bps_rows": (
            sum(population_by_code.values()) == 284_438_600
        ),
        "bps_reported_total_rounding_difference_is_explicit": (
            284_438_800 - sum(population_by_code.values()) == 200
        ),
        "customer_count_matches_policy": customer_sequence == requested_customer_count,
        "all_current_customer_boundaries_pass": current_boundary_rate
        >= float(gates["required_current_boundary_pass_rate"]),
        "all_legacy_customer_boundaries_pass": legacy_boundary_rate
        >= float(gates["required_legacy_boundary_pass_rate"]),
        "all_sites_pass_both_boundaries": all(
            row["current_boundary_pass"] and row["legacy_boundary_pass"] for row in site_checks
        ),
        "population_customer_relationship_is_positive": population_customer_spearman
        >= float(gates["minimum_population_customer_spearman"]),
        "linehaul_quote_distance_pearson_passes": correlations["linehaul"]["pearson"]
        >= float(gates["minimum_quote_distance_pearson"]),
        "linehaul_quote_distance_spearman_passes": correlations["linehaul"]["spearman"]
        >= float(gates["minimum_quote_distance_spearman"]),
        "last_mile_quote_distance_pearson_passes": correlations["last_mile"]["pearson"]
        >= float(gates["minimum_quote_distance_pearson"]),
        "last_mile_quote_distance_spearman_passes": correlations["last_mile"]["spearman"]
        >= float(gates["minimum_quote_distance_spearman"]),
        "current_assignment_is_mostly_nearest_but_imperfect": (
            float(gates["minimum_nearest_assignment_share"])
            <= nearest_share
            <= float(gates["maximum_nearest_assignment_share"])
        ),
        "center_forward_links_are_mostly_nearest_but_imperfect": (
            0.75 <= nearest_parent_count / len(FORWARD_WAREHOUSES) < 1.0
        ),
        "candidate_quote_matrices_are_complete": (
            len(
                {
                    (row["origin_warehouse_id"], row["destination_id"])
                    for row in quote_rows
                    if row["leg_type"] == "linehaul"
                    and row["destination_type"] == "candidate_warehouse"
                }
            )
            == len(CENTRAL_WAREHOUSES) * len(CANDIDATE_LOCATIONS)
            and len(
                {
                    (row["origin_warehouse_id"], row["destination_id"])
                    for row in quote_rows
                    if row["leg_type"] == "last_mile"
                    and row["origin_warehouse_id"].startswith("CAN-")
                }
            )
            == len(CANDIDATE_LOCATIONS) * len(population_rows)
        ),
        "every_existing_warehouse_has_demand": all(
            row["current_annual_demand_units"] > 0 for row in warehouse_rows
        ),
    }
    validation_report = {
        "schema_version": "indonesia_tutorial_validation_report.v1",
        "dataset_id": policy["dataset_id"],
        "release_version": policy["release_version"],
        "seed": policy["seed"],
        "checks": checks,
        "all_passed": all(checks.values()),
        "sources": {
            "source_lock_sha256": _sha256(example_root / "source-lock.json"),
            "population_sha256": _sha256(example_root / "population-2025.csv"),
            "generation_policy_sha256": _sha256(example_root / "generation-policy.json"),
            "world_bank_boundary_sha256": _sha256(current_path),
            "geoboundaries_sha256": _sha256(legacy_path),
            "government_validation": government_check,
        },
        "geometry": {
            "current_province_count": len(current),
            "legacy_province_count": len(legacy),
            "customer_current_boundary_pass_rate": current_boundary_rate,
            "customer_legacy_boundary_pass_rate": legacy_boundary_rate,
            "site_checks": site_checks,
        },
        "demand": {
            "rounded_province_population_sum": sum(population_by_code.values()),
            "bps_reported_indonesia_population_total": 284_438_800,
            "rounding_difference": 284_438_800 - sum(population_by_code.values()),
            "customer_count": requested_customer_count,
            "annual_demand_units": total_demand_units,
            "customer_count_by_province": customer_counts,
            "annual_demand_units_by_province": dict(sorted(demand_by_province.items())),
            "population_customer_spearman": population_customer_spearman,
            "zero_demand_provinces": policy["zero_demand_provinces"],
        },
        "network": {
            "central_warehouse_count": len(CENTRAL_WAREHOUSES),
            "forward_warehouse_count": len(FORWARD_WAREHOUSES),
            "candidate_location_count": len(CANDIDATE_LOCATIONS),
            "nearest_customer_assignment_share": nearest_share,
            "nearest_center_link_share": nearest_parent_count / len(FORWARD_WAREHOUSES),
            "coverage_by_demand_units": {
                f"{day}_day": coverage_units[day] / total_demand_units for day in (1, 2, 3)
            },
            "coverage_by_customer_count": {
                f"{day}_day": coverage_customers[day] / requested_customer_count
                for day in (1, 2, 3)
            },
            "current_cost_idr": {
                "linehaul": linehaul_cost,
                "last_mile": last_mile_cost,
                "total": linehaul_cost + last_mile_cost,
            },
        },
        "quotes": {
            "row_count": len(quote_rows),
            "correlations": correlations,
        },
    }
    validation_path = release_root / "validation-report.json"
    _json_write(validation_path, validation_report)
    shutil.rmtree(scratch_root)

    release_files = [
        (boundary_path, "province_boundaries", 38),
        (customer_path, "customers", requested_customer_count),
        (assignment_path, "customer_assignments", requested_customer_count),
        (warehouse_path, "warehouses", len(warehouse_rows)),
        (link_path, "warehouse_links", len(link_rows)),
        (candidate_path, "candidate_locations", len(candidate_rows)),
        (quote_path, "transport_quotes", len(quote_rows)),
        (policy_path, "planning_policy", 1),
        (validation_path, "validation_report", 1),
    ]
    manifest_files = []
    for path, role, row_count in release_files:
        manifest_files.append(
            {
                "path": path.name,
                "role": role,
                "row_count": row_count,
                "bytes": path.stat().st_size,
                "sha256": _sha256(path),
            }
        )
    content_identity = hashlib.sha256(
        json.dumps(
            manifest_files,
            sort_keys=True,
            separators=(",", ":"),
        ).encode("utf-8")
    ).hexdigest()
    manifest = {
        "schema_version": "workspace_dataset_release.v1",
        "dataset_id": policy["dataset_id"],
        "version": policy["release_version"],
        "content_sha256": content_identity,
        "data_classification": "synthetic_tutorial_data",
        "customer_count": requested_customer_count,
        "files": manifest_files,
        "source_lock": {
            "path": "../../source-lock.json",
            "sha256": _sha256(example_root / "source-lock.json"),
        },
        "attribution": [
            "World Bank Official Boundaries, CC BY 4.0.",
            "geoBoundaries IDN ADM1, ODbL 1.0, used for independent validation.",
            "BPS-Statistics Indonesia, Statistical Yearbook of Indonesia 2025, table 3.1.1.",
        ],
    }
    manifest_path = release_root / "dataset-manifest.json"
    _json_write(manifest_path, manifest)
    if not validation_report["all_passed"]:
        failed = [name for name, passed in checks.items() if not passed]
        raise RuntimeError(f"generated release failed quality gates: {failed}")
    return {
        "release_root": str(release_root),
        "content_sha256": content_identity,
        "customer_count": requested_customer_count,
        "annual_demand_units": total_demand_units,
        "validation_report": str(validation_path),
    }


def _network_geometry_bounds(geometry: dict[str, Any]) -> tuple[float, float, float, float]:
    coordinates = geometry.get("coordinates", [])
    points: list[tuple[float, float]] = []

    def collect(value: Any) -> None:
        if isinstance(value, list) and value and isinstance(value[0], (int, float)):
            points.append((float(value[0]), float(value[1])))
        elif isinstance(value, list):
            for child in value:
                collect(child)

    collect(coordinates)
    if not points:
        raise ValueError("ADM2 geometry has no coordinates")
    longitudes = [point[0] for point in points]
    latitudes = [point[1] for point in points]
    return min(longitudes), min(latitudes), max(longitudes), max(latitudes)


def _network_ring_contains(ring: list[list[float]], longitude: float, latitude: float) -> bool:
    inside = False
    previous = ring[-1]
    for current in ring:
        x1, y1 = float(previous[0]), float(previous[1])
        x2, y2 = float(current[0]), float(current[1])
        if (y1 > latitude) != (y2 > latitude):
            crossing = (x2 - x1) * (latitude - y1) / (y2 - y1) + x1
            if longitude < crossing:
                inside = not inside
        previous = current
    return inside


def _network_geometry_contains(geometry: dict[str, Any], longitude: float, latitude: float) -> bool:
    if geometry.get("type") == "Polygon":
        rings = geometry.get("coordinates", [])
        return (
            bool(rings)
            and _network_ring_contains(rings[0], longitude, latitude)
            and not any(_network_ring_contains(ring, longitude, latitude) for ring in rings[1:])
        )
    if geometry.get("type") == "MultiPolygon":
        return any(
            _network_geometry_contains(
                {"type": "Polygon", "coordinates": polygon}, longitude, latitude
            )
            for polygon in geometry.get("coordinates", [])
        )
    raise ValueError(f"unsupported ADM2 geometry type: {geometry.get('type')}")


def _network_slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")


def _network_find_feature(
    features: list[dict[str, Any]],
    longitude: float,
    latitude: float,
) -> dict[str, Any]:
    matches = [
        feature
        for feature in features
        if _network_geometry_contains(feature["geometry"], longitude, latitude)
    ]
    if len(matches) != 1:
        raise ValueError(f"city point must match exactly one ADM2 feature, got {len(matches)}")
    return matches[0]


def _network_sample_point(
    feature: dict[str, Any], seed: int, fallback: tuple[float, float]
) -> tuple[float, float]:
    geometry = feature["geometry"]
    min_lon, min_lat, max_lon, max_lat = _network_geometry_bounds(geometry)
    rng = random.Random(seed)
    for _ in range(20_000):
        point = (
            round(rng.uniform(min_lon, max_lon), 6),
            round(rng.uniform(min_lat, max_lat), 6),
        )
        if _network_geometry_contains(geometry, point[0], point[1]):
            return point
    if _network_geometry_contains(geometry, fallback[0], fallback[1]):
        return round(fallback[0], 6), round(fallback[1], 6)
    raise ValueError("failed to sample a point inside the matched ADM2 boundary")


def generate_network_fixture(*, root: Path, replace: bool) -> dict[str, Any]:
    """Generate the composable, 50-city tutorial fixtures.

    This path is separate from the historical large-customer release generator:
    it creates the current tutorial sources without changing that old fixture's
    byte-level contract.  The command is explicit so an empty Workspace never
    causes these files to be loaded automatically.
    """
    if root.exists() and replace:
        for generated_name in ("base", "current-coverage-extension", "candidate-extension"):
            shutil.rmtree(root / generated_name, ignore_errors=True)
    if root.exists() and any(
        item.name not in {"source", "base", "current-coverage-extension", "candidate-extension"}
        for item in root.iterdir()
    ):
        raise FileExistsError(f"network fixture path contains unsupported files: {root}")
    base = root / "base"
    current_extension = root / "current-coverage-extension"
    candidate_extension = root / "candidate-extension"
    source_path = root / "source" / "geoboundaries-idn-adm2.geojson"
    if not source_path.exists():
        raise FileNotFoundError(
            f"missing ADM2 source {source_path}; download the locked geoBoundaries snapshot first"
        )
    for directory in (base, current_extension, candidate_extension):
        directory.mkdir(parents=True, exist_ok=True)

    boundary_payload = _json_load(source_path)
    features = boundary_payload.get("features")
    if not isinstance(features, list) or len(features) < 500:
        raise ValueError("geoBoundaries ADM2 source is unexpectedly incomplete")

    demand_rows: list[dict[str, Any]] = []
    by_city_name: dict[str, dict[str, Any]] = {}
    boundary_checks: list[dict[str, Any]] = []
    for index, (city_key, city_name, province_name, population, latitude, longitude) in enumerate(
        TOP_50_CITIES,
        start=1,
    ):
        feature = _network_find_feature(features, longitude, latitude)
        point = _network_sample_point(feature, 20260807 + index, (longitude, latitude))
        city_id = f"IDN-CITY-{index:03d}"
        province_id = f"IDN-PROV-{_network_slug(province_name).upper()}"
        row = {
            "city_id": city_id,
            "city_name": city_name,
            "province_id": province_id,
            "province_name": province_name,
            "admin2_id": feature["properties"]["shapeID"],
            "admin2_name": feature["properties"]["shapeName"],
            "population": population,
            "demand_quantity": math.ceil(population / 1000),
            "longitude": f"{point[0]:.6f}",
            "latitude": f"{point[1]:.6f}",
        }
        demand_rows.append(row)
        by_city_name[city_key] = row
        boundary_checks.append(
            {
                "city_id": city_id,
                "city_name": city_name,
                "admin2_id": row["admin2_id"],
                "inside_admin2": _network_geometry_contains(
                    feature["geometry"], point[0], point[1]
                ),
            }
        )

    demand_fields = [
        "city_id",
        "city_name",
        "province_id",
        "province_name",
        "admin2_id",
        "admin2_name",
        "population",
        "demand_quantity",
        "longitude",
        "latitude",
    ]
    _write_csv(base / "demand-cities.csv", demand_fields, demand_rows)
    _write_csv(
        base / "population-snapshot.csv",
        ["city_id", "city_name", "province_name", "population"],
        [
            {key: row[key] for key in ("city_id", "city_name", "province_name", "population")}
            for row in demand_rows
        ],
    )
    _json_write(
        base / "administrative-areas.json",
        {
            "schema_version": "administrative_catalog.v1",
            "country_code": "ID",
            "admin_level": "ADM2",
            "boundary_source": "geoBoundariesSSCU-3_0_0-IDN-ADM2",
            "rows": [
                {
                    "city_id": row["city_id"],
                    "city_name": row["city_name"],
                    "province_id": row["province_id"],
                    "province_name": row["province_name"],
                    "admin2_id": row["admin2_id"],
                    "admin2_name": row["admin2_name"],
                    "longitude": float(row["longitude"]),
                    "latitude": float(row["latitude"]),
                    "is_province_capital": True,
                }
                for row in demand_rows
            ],
        },
    )

    center_keys = ("JAKARTA", "PALEMBANG", "MEDAN", "SURABAYA", "MAKASSAR")
    cross_keys = ("BEKASI", "BANDUNG", "DEPOK", "TANGERANG", "SEMARANG", "SOUTH_TANGERANG")
    center_rows = []
    warehouse_rows = []
    for warehouse_type, keys in (("center", center_keys), ("cross_docking", cross_keys)):
        for key in keys:
            row = by_city_name[key]
            warehouse_id = f"WH-{warehouse_type.upper()}-{key}"
            center_rows.append((warehouse_id, row))
            warehouse_rows.append(
                {
                    "warehouse_id": warehouse_id,
                    "warehouse_name": f"{row['city_name']} {warehouse_type.replace('_', ' ').title()}",
                    "warehouse_type": warehouse_type,
                    "city_id": row["city_id"],
                    "city_name": row["city_name"],
                    "province_id": row["province_id"],
                    "province_name": row["province_name"],
                    "longitude": row["longitude"],
                    "latitude": row["latitude"],
                    "is_existing": "true",
                    "is_fixed": "true",
                }
            )
    centers = [row for row in warehouse_rows if row["warehouse_type"] == "center"]
    crosses = [row for row in warehouse_rows if row["warehouse_type"] == "cross_docking"]
    for row in crosses:
        parent = min(
            centers,
            key=lambda center: haversine_km(
                (float(center["longitude"]), float(center["latitude"])),
                (float(row["longitude"]), float(row["latitude"])),
            ),
        )
        row["upstream_center_id"] = parent["warehouse_id"]
    _write_csv(
        base / "existing-warehouses.csv",
        [
            "warehouse_id",
            "warehouse_name",
            "warehouse_type",
            "city_id",
            "city_name",
            "province_id",
            "province_name",
            "longitude",
            "latitude",
            "upstream_center_id",
            "is_existing",
            "is_fixed",
        ],
        warehouse_rows,
    )

    quote_rows: list[dict[str, Any]] = []
    route_factor = 1.28
    average_speed_kph = 42.0
    city_points = {
        row["city_id"]: (float(row["longitude"]), float(row["latitude"])) for row in demand_rows
    }
    for warehouse in warehouse_rows:
        origin = (float(warehouse["longitude"]), float(warehouse["latitude"]))
        for demand in demand_rows:
            distance = haversine_km(origin, city_points[demand["city_id"]]) * route_factor
            noise = stable_uniform(
                f"{warehouse['warehouse_id']}:{demand['city_id']}",
                0.985,
                1.015,
            )
            price = round((45_000 + distance * 1_450) * noise / 10_000) * 10_000
            quote_rows.append(
                {
                    "origin_id": warehouse["warehouse_id"],
                    "destination_id": demand["city_id"],
                    "destination_name": demand["city_name"],
                    "layer": "last_mile",
                    "distance_km": f"{distance:.3f}",
                    "duration_hours": f"{distance / average_speed_kph:.3f}",
                    "price_per_vehicle": price,
                    "currency": "IDR",
                    "vehicle_capacity": 1,
                    "method": "haversine",
                }
            )
    linehaul_rows = []
    for cross in crosses:
        for parent in centers:
            distance = (
                haversine_km(
                    (float(parent["longitude"]), float(parent["latitude"])),
                    (float(cross["longitude"]), float(cross["latitude"])),
                )
                * route_factor
            )
            price = round((125_000 + distance * 1_100) / 10_000) * 10_000
            linehaul_rows.append(
                {
                    "origin_id": parent["warehouse_id"],
                    "destination_id": cross["warehouse_id"],
                    "destination_name": cross["city_name"],
                    "layer": "linehaul",
                    "distance_km": f"{distance:.3f}",
                    "duration_hours": f"{distance / average_speed_kph:.3f}",
                    "price_per_vehicle": price,
                    "currency": "IDR",
                    "vehicle_capacity": 1,
                    "method": "haversine",
                }
            )
    quote_rows.extend(linehaul_rows)
    _write_csv(
        base / "route-quotes.csv",
        [
            "origin_id",
            "destination_id",
            "destination_name",
            "layer",
            "distance_km",
            "duration_hours",
            "price_per_vehicle",
            "currency",
            "vehicle_capacity",
            "method",
        ],
        quote_rows,
    )

    candidate_keys = (
        "PADANG",
        "PEKANBARU",
        "JAMBI",
        "BANDAR_LAMPUNG",
        "PONTIANAK",
        "BANJARMASIN",
        "BALIKPAPAN",
        "MANADO",
        "PALU",
        "KENDARI",
        "MATARAM",
        "KUPANG",
    )
    candidate_rows = []
    for key in candidate_keys:
        row = by_city_name[key]
        nearest_center = min(
            centers,
            key=lambda center: haversine_km(
                (float(center["longitude"]), float(center["latitude"])),
                (float(row["longitude"]), float(row["latitude"])),
            ),
        )
        candidate_rows.append(
            {
                "warehouse_id": f"WH-CANDIDATE-{key}",
                "warehouse_name": f"{row['city_name']} Candidate Cross Docking",
                "warehouse_type": "cross_docking",
                "city_id": row["city_id"],
                "city_name": row["city_name"],
                "province_id": row["province_id"],
                "province_name": row["province_name"],
                "longitude": row["longitude"],
                "latitude": row["latitude"],
                "upstream_center_id": nearest_center["warehouse_id"],
                "is_existing": "false",
                "is_fixed": "false",
            }
        )
    _write_csv(base / "candidate-warehouses.csv", list(candidate_rows[0]), candidate_rows)
    _write_csv(
        candidate_extension / "candidate-warehouses.csv", list(candidate_rows[0]), candidate_rows
    )

    cross_ids = [row["warehouse_id"] for row in crosses]
    current_rows = []
    for index, demand in enumerate(demand_rows):
        nearest = sorted(
            cross_ids,
            key=lambda warehouse_id: haversine_km(
                next(
                    (float(row["longitude"]), float(row["latitude"]))
                    for row in crosses
                    if row["warehouse_id"] == warehouse_id
                ),
                city_points[demand["city_id"]],
            ),
        )
        selected = nearest[1] if index % 5 == 0 else nearest[0]
        parent = next(
            row["upstream_center_id"] for row in crosses if row["warehouse_id"] == selected
        )
        current_rows.append(
            {
                "demand_city_id": demand["city_id"],
                "serving_warehouse_id": selected,
                "upstream_center_id": parent,
            }
        )
    _write_csv(
        current_extension / "current-coverage.csv",
        ["demand_city_id", "serving_warehouse_id", "upstream_center_id"],
        current_rows,
    )

    files_for_lock = [
        base / name
        for name in (
            "demand-cities.csv",
            "existing-warehouses.csv",
            "route-quotes.csv",
            "administrative-areas.json",
            "candidate-warehouses.csv",
            "population-snapshot.csv",
        )
    ]
    source_lock = {
        "schema_version": "tutorial_source_lock.v1",
        "generated_at": "2026-08-07",
        "generator": "generate_indonesia_tutorial_data.py:network-fixture-v1",
        "sources": [
            {
                "source_id": "geoboundaries-idn-adm2",
                "url": "https://www.geoboundaries.org/data/geoBoundaries-3_0_0/IDN/ADM2/geoBoundaries-3_0_0-IDN-ADM2.geojson",
                "fetched_at": "2026-08-07",
                "license": "Other - Humanitarian; source metadata identifies BPS and OCHA ROAP",
                "path": "../source/geoboundaries-idn-adm2.geojson",
                "sha256": _sha256(source_path),
            },
            {
                "source_id": "bps-city-population-snapshot",
                "url": "https://www.bps.go.id/",
                "fetched_at": "2026-08-07",
                "license": "BPS public statistics; curated rounded city snapshot for tutorial use",
                "path": "population-snapshot.csv",
                "sha256": _sha256(base / "population-snapshot.csv"),
            },
        ],
        "generated_file_sha256": {path.name: _sha256(path) for path in files_for_lock},
    }
    _json_write(base / "source-lock.json", source_lock)
    quote_distances = [float(row["distance_km"]) for row in quote_rows[: 11 * len(demand_rows)]]
    quote_prices = [float(row["price_per_vehicle"]) for row in quote_rows[: 11 * len(demand_rows)]]
    validation = {
        "schema_version": "indonesia_network_fixture_validation.v1",
        "boundary_source": {
            "feature_count": len(features),
            "sha256": _sha256(source_path),
            "all_50_points_inside_adm2": all(row["inside_admin2"] for row in boundary_checks),
            "boundary_checks": boundary_checks,
        },
        "demand": {
            "city_count": len(demand_rows),
            "demand_formula": "ceil(population / 1000)",
            "formula_pass": all(
                row["demand_quantity"] == math.ceil(row["population"] / 1000) for row in demand_rows
            ),
            "province_count": len({row["province_id"] for row in demand_rows}),
        },
        "network": {
            "existing_warehouse_count": len(warehouse_rows),
            "center_count": len(centers),
            "cross_docking_count": len(crosses),
            "all_existing_points_inside_declared_adm2": all(
                any(row["city_id"] == warehouse["city_id"] for row in demand_rows)
                for warehouse in warehouse_rows
            ),
        },
        "quotes": {
            "warehouse_to_demand_row_count": 11 * len(demand_rows),
            "linehaul_row_count": len(linehaul_rows),
            "total_row_count": len(quote_rows),
            "spearman_distance_price": spearman_correlation(quote_distances, quote_prices),
            "spearman_pass": spearman_correlation(quote_distances, quote_prices) >= 0.85,
        },
        "current_coverage_extension": {
            "row_count": len(current_rows),
            "unique_demand_cities": len({row["demand_city_id"] for row in current_rows}),
        },
        "all_passed": all(
            (
                len(demand_rows) == 50,
                all(
                    row["demand_quantity"] == math.ceil(row["population"] / 1000)
                    for row in demand_rows
                ),
                all(row["inside_admin2"] for row in boundary_checks),
                len(warehouse_rows) == 11,
                len(centers) == 5,
                len(crosses) == 6,
                spearman_correlation(quote_distances, quote_prices) >= 0.85,
                len(quote_rows[: 11 * len(demand_rows)]) == 550,
                len(current_rows) == 50,
            )
        ),
    }
    _json_write(base / "validation-report.json", validation)
    manifest_specs = [
        ("demand-cities.csv", "demand_cities", len(demand_rows)),
        ("existing-warehouses.csv", "existing_warehouses", len(warehouse_rows)),
        ("route-quotes.csv", "route_quotes", len(quote_rows)),
        ("administrative-areas.json", "administrative_catalog", len(demand_rows)),
        ("candidate-warehouses.csv", "candidate_warehouses", len(candidate_rows)),
        ("population-snapshot.csv", "population_snapshot", len(demand_rows)),
        ("source-lock.json", "source_lock", 1),
        ("validation-report.json", "validation_report", 1),
    ]
    manifest_files = [
        {
            "path": name,
            "role": role,
            "row_count": row_count,
            "bytes": (base / name).stat().st_size,
            "sha256": _sha256(base / name),
        }
        for name, role, row_count in manifest_specs
    ]
    content_identity = hashlib.sha256(
        json.dumps(manifest_files, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    _json_write(
        base / "dataset-manifest.json",
        {
            "schema_version": "workspace_dataset_release.v1",
            "dataset_id": NETWORK_DATASET_ID,
            "version": NETWORK_DATASET_VERSION,
            "content_sha256": content_identity,
            "data_classification": "synthetic_tutorial_data",
            "customer_count": len(demand_rows),
            "files": manifest_files,
            "source_lock": {
                "path": "source-lock.json",
                "sha256": _sha256(base / "source-lock.json"),
            },
            "attribution": [
                "geoBoundaries IDN ADM2, used to validate generated points.",
                "BPS-Statistics Indonesia, curated population snapshot for tutorial use.",
            ],
        },
    )
    return {
        "root": str(root),
        "demand_city_count": len(demand_rows),
        "existing_warehouse_count": len(warehouse_rows),
        "quote_count": len(quote_rows),
        "all_passed": validation["all_passed"],
        "source_sha256": _sha256(source_path),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--example-root", type=Path, default=EXAMPLE_ROOT)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_RELEASE_ROOT)
    parser.add_argument("--customer-count", type=int)
    parser.add_argument("--replace", action="store_true")
    parser.add_argument("--download-missing-sources", action="store_true")
    parser.add_argument("--refresh-external-validation", action="store_true")
    parser.add_argument(
        "--network-fixture",
        action="store_true",
        help="Generate the current composable 50-city network tutorial fixture.",
    )
    parser.add_argument(
        "--network-output-dir",
        type=Path,
        default=NETWORK_EXAMPLE_ROOT,
    )
    args = parser.parse_args()
    if args.network_fixture:
        result = generate_network_fixture(
            root=args.network_output_dir.resolve(), replace=args.replace
        )
    else:
        result = generate(
            example_root=args.example_root.resolve(),
            release_root=args.output_dir.resolve(),
            customer_count=args.customer_count,
            replace=args.replace,
            download_missing_sources=args.download_missing_sources,
            refresh_external_validation=args.refresh_external_validation,
        )
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()

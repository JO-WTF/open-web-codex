"""Small deterministic geospatial helpers used by the tutorial dataset and MCP."""

from __future__ import annotations

import bisect
import hashlib
import math
import random
from dataclasses import dataclass
from typing import Any

Coordinate = tuple[float, float]


def haversine_km(origin: Coordinate, destination: Coordinate) -> float:
    """Return great-circle distance in kilometers for longitude/latitude pairs."""
    lon1, lat1 = origin
    lon2, lat2 = destination
    radius_km = 6371.0088
    phi1 = math.radians(lat1)
    phi2 = math.radians(lat2)
    delta_phi = math.radians(lat2 - lat1)
    delta_lambda = math.radians(lon2 - lon1)
    value = (
        math.sin(delta_phi / 2) ** 2
        + math.cos(phi1) * math.cos(phi2) * math.sin(delta_lambda / 2) ** 2
    )
    return 2 * radius_km * math.asin(min(1.0, math.sqrt(value)))


def pearson_correlation(xs: list[float], ys: list[float]) -> float:
    if len(xs) != len(ys) or len(xs) < 2:
        raise ValueError("correlation requires equal lists with at least two values")
    mean_x = sum(xs) / len(xs)
    mean_y = sum(ys) / len(ys)
    covariance = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys, strict=True))
    variance_x = sum((x - mean_x) ** 2 for x in xs)
    variance_y = sum((y - mean_y) ** 2 for y in ys)
    denominator = math.sqrt(variance_x * variance_y)
    if denominator == 0:
        raise ValueError("correlation is undefined for a constant series")
    return covariance / denominator


def _ranks(values: list[float]) -> list[float]:
    ordered = sorted(enumerate(values), key=lambda item: item[1])
    ranks = [0.0] * len(values)
    index = 0
    while index < len(ordered):
        end = index + 1
        while end < len(ordered) and ordered[end][1] == ordered[index][1]:
            end += 1
        average_rank = (index + 1 + end) / 2
        for original_index, _ in ordered[index:end]:
            ranks[original_index] = average_rank
        index = end
    return ranks


def spearman_correlation(xs: list[float], ys: list[float]) -> float:
    return pearson_correlation(_ranks(xs), _ranks(ys))


def stable_uniform(key: str, low: float, high: float) -> float:
    if low > high:
        raise ValueError("low must not exceed high")
    digest = hashlib.sha256(key.encode("utf-8")).digest()
    unit = int.from_bytes(digest[:8], "big") / ((1 << 64) - 1)
    return low + (high - low) * unit


def _ring_area(ring: tuple[Coordinate, ...]) -> float:
    return abs(
        sum(x1 * y2 - x2 * y1 for (x1, y1), (x2, y2) in zip(ring, ring[1:] + ring[:1], strict=True))
        / 2
    )


def _point_on_segment(
    point: Coordinate,
    start: Coordinate,
    end: Coordinate,
    *,
    tolerance: float = 1e-10,
) -> bool:
    px, py = point
    x1, y1 = start
    x2, y2 = end
    cross = (px - x1) * (y2 - y1) - (py - y1) * (x2 - x1)
    if abs(cross) > tolerance:
        return False
    return (
        min(x1, x2) - tolerance <= px <= max(x1, x2) + tolerance
        and min(y1, y2) - tolerance <= py <= max(y1, y2) + tolerance
    )


def _point_in_ring(point: Coordinate, ring: tuple[Coordinate, ...]) -> bool:
    inside = False
    px, py = point
    previous = ring[-1]
    for current in ring:
        if _point_on_segment(point, previous, current):
            return True
        x1, y1 = previous
        x2, y2 = current
        if (y1 > py) != (y2 > py):
            crossing_x = (x2 - x1) * (py - y1) / (y2 - y1) + x1
            if px < crossing_x:
                inside = not inside
        previous = current
    return inside


def _normalize_ring(raw_ring: list[list[float]]) -> tuple[Coordinate, ...]:
    ring = tuple((float(value[0]), float(value[1])) for value in raw_ring)
    if len(ring) < 4:
        raise ValueError("polygon rings require at least four coordinates")
    if ring[0] == ring[-1]:
        ring = ring[:-1]
    if len(ring) < 3:
        raise ValueError("polygon rings require at least three distinct coordinates")
    return ring


@dataclass(frozen=True)
class BoundaryPart:
    outer: tuple[Coordinate, ...]
    holes: tuple[tuple[Coordinate, ...], ...]
    bounds: tuple[float, float, float, float]
    area: float

    @classmethod
    def from_polygon(cls, raw_polygon: list[list[list[float]]]) -> BoundaryPart:
        if not raw_polygon:
            raise ValueError("polygon has no rings")
        outer = _normalize_ring(raw_polygon[0])
        holes = tuple(_normalize_ring(ring) for ring in raw_polygon[1:])
        longitudes = [point[0] for point in outer]
        latitudes = [point[1] for point in outer]
        area = _ring_area(outer) - sum(_ring_area(hole) for hole in holes)
        if area <= 0:
            raise ValueError("polygon has no positive area")
        return cls(
            outer=outer,
            holes=holes,
            bounds=(
                min(longitudes),
                min(latitudes),
                max(longitudes),
                max(latitudes),
            ),
            area=area,
        )

    def contains(self, point: Coordinate) -> bool:
        longitude, latitude = point
        min_lon, min_lat, max_lon, max_lat = self.bounds
        if not (min_lon <= longitude <= max_lon and min_lat <= latitude <= max_lat):
            return False
        return _point_in_ring(point, self.outer) and not any(
            _point_in_ring(point, hole) for hole in self.holes
        )

    def sample(self, rng: random.Random, *, maximum_attempts: int = 5000) -> Coordinate:
        min_lon, min_lat, max_lon, max_lat = self.bounds
        for _ in range(maximum_attempts):
            point = (
                rng.uniform(min_lon, max_lon),
                rng.uniform(min_lat, max_lat),
            )
            if self.contains(point):
                return point
        raise RuntimeError("failed to sample an interior polygon point")


@dataclass(frozen=True)
class ProvinceBoundary:
    code: str
    name: str
    parts: tuple[BoundaryPart, ...]
    cumulative_areas: tuple[float, ...]
    total_area: float
    bounds: tuple[float, float, float, float]

    @classmethod
    def from_feature(
        cls,
        feature: dict[str, Any],
        *,
        code_field: str,
        name_field: str,
    ) -> ProvinceBoundary:
        properties = feature.get("properties") or {}
        code = str(properties[code_field])
        name = str(properties[name_field])
        geometry = feature.get("geometry") or {}
        geometry_type = geometry.get("type")
        coordinates = geometry.get("coordinates")
        if geometry_type == "Polygon":
            polygons = [coordinates]
        elif geometry_type == "MultiPolygon":
            polygons = coordinates
        else:
            raise ValueError(f"unsupported boundary geometry {geometry_type!r}")
        parts = tuple(BoundaryPart.from_polygon(polygon) for polygon in polygons)
        cumulative: list[float] = []
        total = 0.0
        for part in parts:
            total += part.area
            cumulative.append(total)
        return cls(
            code=code,
            name=name,
            parts=parts,
            cumulative_areas=tuple(cumulative),
            total_area=total,
            bounds=(
                min(part.bounds[0] for part in parts),
                min(part.bounds[1] for part in parts),
                max(part.bounds[2] for part in parts),
                max(part.bounds[3] for part in parts),
            ),
        )

    def contains(self, point: Coordinate) -> bool:
        return any(part.contains(point) for part in self.parts)

    def sample(self, rng: random.Random) -> Coordinate:
        value = rng.random() * self.total_area
        index = bisect.bisect_left(self.cumulative_areas, value)
        return self.parts[min(index, len(self.parts) - 1)].sample(rng)

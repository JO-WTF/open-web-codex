import { useMemo, useState } from "react";
import Expand from "lucide-react/dist/esm/icons/expand";
import MapPinned from "lucide-react/dist/esm/icons/map-pinned";
import type { MapBounds, MapReplyCard as MapReplyCardData } from "../../../utils/replyCards";

type Props = {
  card: MapReplyCardData;
};

type FeatureCollection = {
  type: "FeatureCollection";
  bbox?: unknown;
  features: Array<Record<string, unknown>>;
};

type ProjectedPoint = { x: number; y: number; label?: string; color?: string };

function statusLabel(status: MapReplyCardData["status"]) {
  if (status === "ready") return "Ready";
  if (status === "error") return "Failed";
  return "Waiting for GeoJSON Artifact";
}

function extendCoordinateBounds(bounds: MapBounds | null, longitude: number, latitude: number): MapBounds {
  if (!bounds) return [longitude, latitude, longitude, latitude];
  return [
    Math.min(bounds[0], longitude),
    Math.min(bounds[1], latitude),
    Math.max(bounds[2], longitude),
    Math.max(bounds[3], latitude),
  ];
}

function collectGeoJsonCoordinates(value: unknown, visit: (longitude: number, latitude: number) => void) {
  if (Array.isArray(value)) {
    if (
      value.length >= 2
      && typeof value[0] === "number"
      && Number.isFinite(value[0])
      && typeof value[1] === "number"
      && Number.isFinite(value[1])
    ) {
      visit(value[0], value[1]);
      return;
    }
    for (const entry of value) collectGeoJsonCoordinates(entry, visit);
    return;
  }
  if (!value || typeof value !== "object") return;
  const record = value as Record<string, unknown>;
  if (record.type === "FeatureCollection") collectGeoJsonCoordinates(record.features, visit);
  else if (record.type === "Feature") collectGeoJsonCoordinates(record.geometry, visit);
  else if (record.type === "GeometryCollection") collectGeoJsonCoordinates(record.geometries, visit);
  else collectGeoJsonCoordinates(record.coordinates, visit);
}

export function dataBoundsForCard(card: MapReplyCardData): MapBounds | null {
  let bounds: MapBounds | null = null;
  const extend = (longitude: number, latitude: number) => {
    if (longitude < -180 || longitude > 180 || latitude < -90 || latitude > 90) return;
    bounds = extendCoordinateBounds(bounds, longitude, latitude);
  };
  for (const point of card.points ?? []) extend(point.longitude, point.latitude);
  for (const line of card.lines ?? []) {
    for (const [longitude, latitude] of line.coordinates) extend(longitude, latitude);
  }
  for (const polygon of card.polygons ?? []) {
    for (const ring of polygon.coordinates) {
      for (const [longitude, latitude] of ring) extend(longitude, latitude);
    }
  }
  collectGeoJsonCoordinates(card.geojson, extend);
  if (!bounds) return null;
  if (bounds[0] === bounds[2] && bounds[1] === bounds[3]) {
    const padding = 0.08;
    return [bounds[0] - padding, bounds[1] - padding, bounds[2] + padding, bounds[3] + padding];
  }
  return bounds;
}

function isFeatureCollection(value: unknown): value is FeatureCollection {
  return Boolean(value && typeof value === "object" && !Array.isArray(value) && (value as { type?: unknown }).type === "FeatureCollection" && Array.isArray((value as { features?: unknown }).features));
}

function featureCollectionForCard(card: MapReplyCardData): FeatureCollection | null {
  if (isFeatureCollection(card.geojson)) return card.geojson;
  const features: FeatureCollection["features"] = [];
  for (const point of card.points ?? []) {
    features.push({
      type: "Feature",
      properties: { id: point.id, label: point.label, description: point.description, color: point.color, kind: "point" },
      geometry: { type: "Point", coordinates: [point.longitude, point.latitude] },
    });
  }
  for (const line of card.lines ?? []) {
    features.push({
      type: "Feature",
      properties: { id: line.id, label: line.label, color: line.color, kind: "line" },
      geometry: { type: "LineString", coordinates: line.coordinates },
    });
  }
  for (const polygon of card.polygons ?? []) {
    features.push({
      type: "Feature",
      properties: { id: polygon.id, label: polygon.label, color: polygon.color, kind: "polygon" },
      geometry: { type: "Polygon", coordinates: polygon.coordinates },
    });
  }
  return features.length ? { type: "FeatureCollection", features } : null;
}

function collectFeaturePoints(collection: FeatureCollection | null): Array<{ longitude: number; latitude: number; label?: string; color?: string }> {
  const points: Array<{ longitude: number; latitude: number; label?: string; color?: string }> = [];
  for (const feature of collection?.features ?? []) {
    const geometry = feature.geometry as Record<string, unknown> | undefined;
    const properties = feature.properties as Record<string, unknown> | undefined;
    collectGeoJsonCoordinates(geometry, (longitude, latitude) => {
      points.push({
        longitude,
        latitude,
        label: typeof properties?.label === "string" ? properties.label : undefined,
        color: typeof properties?.color === "string" ? properties.color : undefined,
      });
    });
  }
  return points.slice(0, 150);
}

function projectPoint(longitude: number, latitude: number, bounds: MapBounds): ProjectedPoint {
  const [west, south, east, north] = bounds;
  const width = Math.max(east - west, 0.000001);
  const height = Math.max(north - south, 0.000001);
  return {
    x: 24 + ((longitude - west) / width) * 312,
    y: 196 - ((latitude - south) / height) * 152,
  };
}

function MapPreview({ card }: { card: MapReplyCardData }) {
  const collection = useMemo(() => featureCollectionForCard(card), [card]);
  const bounds = card.bbox ?? dataBoundsForCard(card);
  const points = useMemo(() => {
    if (!bounds) return [];
    return collectFeaturePoints(collection).map((point) => ({
      ...projectPoint(point.longitude, point.latitude, bounds),
      label: point.label,
      color: point.color,
    }));
  }, [bounds, collection]);

  if (!bounds || points.length === 0) {
    return (
      <div className="web-map-card-canvas" role="status" aria-label="Map data pending">
        <MapPinned size={28} aria-hidden="true" />
        <strong>等待地图数据</strong>
        <span>模型已返回 map-card 标记，等待 Artifact 或内联几何数据。</span>
      </div>
    );
  }

  return (
    <svg className="web-map-card-svg" viewBox="0 0 360 220" role="img" aria-label="Map card preview">
      <rect width="360" height="220" rx="18" />
      <path d="M28 64 C94 28 136 104 198 72 S288 62 332 36" />
      <path d="M22 154 C76 124 116 184 176 148 S260 114 340 160" />
      {points.map((point, index) => (
        <g key={`${point.x}-${point.y}-${index}`}>
          <circle cx={point.x} cy={point.y} r="6" style={{ fill: point.color ?? "#f97316" }} />
          <circle cx={point.x} cy={point.y} r="10" />
          {point.label && <text x={point.x + 10} y={point.y - 8}>{point.label}</text>}
        </g>
      ))}
    </svg>
  );
}

export default function MapReplyCard({ card }: Props) {
  const [fullscreen, setFullscreen] = useState(false);
  const status = card.status ?? "loading";
  const detail = card.summary ?? card.fallbackText ?? "地图卡片已由平台识别；第三方 Provider 可通过 map-card 标记和 MCP 地理工具提供数据。";
  const body = (fullscreenBody = false) => (
    <div className={`web-map-card is-${status}`} role="group" aria-label={`Map card: ${card.title}`}>
      <div className="web-map-card-header">
        <div className="web-map-card-title"><MapPinned size={16} aria-hidden="true" /><span>{card.title}</span></div>
        <span className="web-map-card-status">{statusLabel(status)}</span>
        {!fullscreenBody && (
          <button type="button" className="web-map-card-fullscreen" onClick={() => setFullscreen(true)} aria-label="Open map card fullscreen">
            <Expand size={16} aria-hidden="true" />
            <span>全屏</span>
          </button>
        )}
      </div>
      <MapPreview card={card} />
      <div className="web-map-card-body">
        <p>{detail}</p>
        <dl>
          {card.intent && <><dt>Intent</dt><dd>{card.intent}</dd></>}
          {card.inputRef && <><dt>Input ref</dt><dd>{card.inputRef}</dd></>}
          {card.artifactId && <><dt>Artifact</dt><dd>{card.artifactId}</dd></>}
          {card.points?.length ? <><dt>Points</dt><dd>{card.points.length}</dd></> : null}
          {card.lines?.length ? <><dt>Lines</dt><dd>{card.lines.length}</dd></> : null}
          {card.polygons?.length ? <><dt>Polygons</dt><dd>{card.polygons.length}</dd></> : null}
        </dl>
      </div>
    </div>
  );
  return (
    <>
      {body(false)}
      {fullscreen && (
        <div className="web-map-card-modal" role="dialog" aria-modal="true" aria-label={`Fullscreen map card: ${card.title}`}>
          <div className="web-map-card-modal-backdrop" onClick={() => setFullscreen(false)} />
          <div className="web-map-card-modal-panel">
            <button type="button" className="web-map-card-modal-close" onClick={() => setFullscreen(false)}>Close</button>
            {body(true)}
          </div>
        </div>
      )}
    </>
  );
}

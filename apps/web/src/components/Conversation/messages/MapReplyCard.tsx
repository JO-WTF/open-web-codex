import { useEffect, useMemo, useRef, useState } from "react";
import "mapbox-gl/dist/mapbox-gl.css";
import Expand from "lucide-react/dist/esm/icons/expand";
import MapPinned from "lucide-react/dist/esm/icons/map-pinned";
import type { Map as MapboxMap, LngLatBoundsLike } from "mapbox-gl";
import type { MapReplyCard as MapReplyCardData } from "../../../utils/replyCards";

type Props = {
  card: MapReplyCardData;
};

type FeatureCollection = {
  type: "FeatureCollection";
  bbox?: unknown;
  features: Array<Record<string, unknown>>;
};

const MAPBOX_TOKEN = import.meta.env.VITE_MAPBOX_ACCESS_TOKEN ?? import.meta.env.VITE_MAPBOX_TOKEN ?? "";

function statusLabel(status: MapReplyCardData["status"]) {
  if (status === "ready") return "Ready";
  if (status === "error") return "Failed";
  return "Waiting for GeoJSON Artifact";
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

function boundsFromBbox(value: unknown): LngLatBoundsLike | null {
  if (!Array.isArray(value) || value.length < 4) return null;
  const [west, south, east, north] = value;
  if (![west, south, east, north].every((entry) => typeof entry === "number" && Number.isFinite(entry))) return null;
  return [[west, south], [east, north]];
}

function MapCanvas({ card, fullscreen = false }: { card: MapReplyCardData; fullscreen?: boolean }) {
  const mapEl = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MapboxMap | null>(null);
  const geojson = useMemo(() => featureCollectionForCard(card), [card]);
  const canRenderMap = Boolean(MAPBOX_TOKEN && geojson);

  useEffect(() => {
    if (!canRenderMap || !mapEl.current || !geojson) return;
    let cancelled = false;
    void import("mapbox-gl").then((module) => {
      if (cancelled || !mapEl.current) return;
      const mapboxgl = module.default;
      mapboxgl.accessToken = MAPBOX_TOKEN;
      const bounds = boundsFromBbox(card.bbox ?? geojson.bbox);
      const map = new mapboxgl.Map({
        container: mapEl.current,
        style: "mapbox://styles/mapbox/streets-v12",
        center: card.center ? [card.center.longitude, card.center.latitude] : [0, 0],
        zoom: card.zoom ?? 2,
      });
      mapRef.current = map;
      map.addControl(new mapboxgl.NavigationControl({ showCompass: false }), "top-right");
      map.on("load", () => {
        if (!map.getSource("reply-card-geojson")) {
          map.addSource("reply-card-geojson", { type: "geojson", data: geojson as never });
        }
        map.addLayer({ id: "reply-card-polygons", type: "fill", source: "reply-card-geojson", filter: ["==", ["geometry-type"], "Polygon"], paint: { "fill-color": ["coalesce", ["get", "color"], "#0891b2"], "fill-opacity": 0.28 } });
        map.addLayer({ id: "reply-card-lines", type: "line", source: "reply-card-geojson", filter: ["in", ["geometry-type"], ["literal", ["LineString", "Polygon"]]], paint: { "line-color": ["coalesce", ["get", "color"], "#2563eb"], "line-width": 4 } });
        map.addLayer({ id: "reply-card-points", type: "circle", source: "reply-card-geojson", filter: ["==", ["geometry-type"], "Point"], paint: { "circle-color": ["coalesce", ["get", "color"], "#f97316"], "circle-radius": 6, "circle-stroke-color": "#ffffff", "circle-stroke-width": 2 } });
        if (bounds) map.fitBounds(bounds, { padding: fullscreen ? 72 : 36, maxZoom: card.zoom ?? 15, duration: 0 });
      });
    });
    return () => {
      cancelled = true;
      mapRef.current?.remove();
      mapRef.current = null;
    };
  }, [canRenderMap, card.bbox, card.center, card.zoom, fullscreen, geojson]);

  if (canRenderMap) return <div ref={mapEl} className="web-map-card-mapbox" aria-label="Mapbox map" />;
  return (
    <div className="web-map-card-canvas" aria-hidden="true">
      <div className="web-map-card-route" />
      <span className="web-map-card-pin is-a" />
      <span className="web-map-card-pin is-b" />
    </div>
  );
}

export default function MapReplyCard({ card }: Props) {
  const [fullscreen, setFullscreen] = useState(false);
  const status = card.status ?? "loading";
  const detail = card.summary ?? card.fallbackText ?? (MAPBOX_TOKEN ? "地图卡片将在平台完成 GeoJSON Artifact hydration 后渲染。" : "未配置 VITE_MAPBOX_ACCESS_TOKEN，地图以占位卡片显示。");
  const body = (fullscreenBody = false) => (
    <div className={`web-map-card is-${status}`} role="group" aria-label={`Map card: ${card.title}`}>
      <div className="web-map-card-header">
        <div className="web-map-card-title"><MapPinned size={16} aria-hidden="true" /><span>{card.title}</span></div>
        <span className="web-map-card-status">{statusLabel(status)}</span>
        {!fullscreenBody && <button type="button" className="web-map-card-fullscreen" onClick={() => setFullscreen(true)} aria-label="Open map card fullscreen"><Expand size={14} aria-hidden="true" /></button>}
      </div>
      <MapCanvas card={card} fullscreen={fullscreenBody} />
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

import { memo, useEffect, useMemo, useRef, useState } from "react";
import Expand from "lucide-react/dist/esm/icons/expand";
import KeyRound from "lucide-react/dist/esm/icons/key-round";
import MapPinned from "lucide-react/dist/esm/icons/map-pinned";
import type {
  LngLatBoundsLike,
  Map as MapboxMap,
} from "mapbox-gl";
import "mapbox-gl/dist/mapbox-gl.css";
import type {
  MapBounds,
  MapReplyCard as MapReplyCardData,
} from "../../../utils/replyCards";
import { useMapsConfiguration } from "../../../services/mapsConfiguration";
import MapsConfigurationModal from "./MapsConfigurationModal";

type Props = {
  card: MapReplyCardData;
};

export function sameMapReplyCard(
  left: MapReplyCardData,
  right: MapReplyCardData,
): boolean {
  if (left === right) return true;
  return JSON.stringify(left) === JSON.stringify(right);
}

type FeatureCollection = {
  type: "FeatureCollection";
  bbox?: unknown;
  features: Array<Record<string, unknown>>;
};

type MapLoadState = "loading" | "ready" | "error";

function statusLabel(status: MapReplyCardData["status"]) {
  if (status === "ready") return "Ready";
  if (status === "error") return "Failed";
  return "Waiting for GeoJSON Artifact";
}

function extendCoordinateBounds(
  bounds: MapBounds | null,
  longitude: number,
  latitude: number,
): MapBounds {
  if (!bounds) return [longitude, latitude, longitude, latitude];
  return [
    Math.min(bounds[0], longitude),
    Math.min(bounds[1], latitude),
    Math.max(bounds[2], longitude),
    Math.max(bounds[3], latitude),
  ];
}

function collectGeoJsonCoordinates(
  value: unknown,
  visit: (longitude: number, latitude: number) => void,
) {
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
  if (record.type === "FeatureCollection") {
    collectGeoJsonCoordinates(record.features, visit);
  } else if (record.type === "Feature") {
    collectGeoJsonCoordinates(record.geometry, visit);
  } else if (record.type === "GeometryCollection") {
    collectGeoJsonCoordinates(record.geometries, visit);
  } else {
    collectGeoJsonCoordinates(record.coordinates, visit);
  }
}

export function dataBoundsForCard(card: MapReplyCardData): MapBounds | null {
  let bounds: MapBounds | null = null;
  const extend = (longitude: number, latitude: number) => {
    if (
      longitude < -180
      || longitude > 180
      || latitude < -90
      || latitude > 90
    ) {
      return;
    }
    bounds = extendCoordinateBounds(bounds, longitude, latitude);
  };
  for (const point of card.points ?? []) {
    extend(point.longitude, point.latitude);
  }
  for (const line of card.lines ?? []) {
    for (const [longitude, latitude] of line.coordinates) {
      extend(longitude, latitude);
    }
  }
  for (const polygon of card.polygons ?? []) {
    for (const ring of polygon.coordinates) {
      for (const [longitude, latitude] of ring) {
        extend(longitude, latitude);
      }
    }
  }
  collectGeoJsonCoordinates(card.geojson, extend);
  if (!bounds) return null;
  if (bounds[0] === bounds[2] && bounds[1] === bounds[3]) {
    const padding = 0.08;
    return [
      bounds[0] - padding,
      bounds[1] - padding,
      bounds[2] + padding,
      bounds[3] + padding,
    ];
  }
  return bounds;
}

function isFeatureCollection(value: unknown): value is FeatureCollection {
  return Boolean(
    value
    && typeof value === "object"
    && !Array.isArray(value)
    && (value as { type?: unknown }).type === "FeatureCollection"
    && Array.isArray((value as { features?: unknown }).features),
  );
}

export function featureCollectionForCard(
  card: MapReplyCardData,
): FeatureCollection | null {
  if (isFeatureCollection(card.geojson)) return card.geojson;
  const features: FeatureCollection["features"] = [];
  for (const point of card.points ?? []) {
    features.push({
      type: "Feature",
      properties: {
        id: point.id,
        label: point.label,
        description: point.description,
        color: point.color,
        kind: "point",
      },
      geometry: {
        type: "Point",
        coordinates: [point.longitude, point.latitude],
      },
    });
  }
  for (const line of card.lines ?? []) {
    features.push({
      type: "Feature",
      properties: {
        id: line.id,
        label: line.label,
        color: line.color,
        kind: "line",
      },
      geometry: { type: "LineString", coordinates: line.coordinates },
    });
  }
  for (const polygon of card.polygons ?? []) {
    features.push({
      type: "Feature",
      properties: {
        id: polygon.id,
        label: polygon.label,
        color: polygon.color,
        kind: "polygon",
      },
      geometry: { type: "Polygon", coordinates: polygon.coordinates },
    });
  }
  return features.length
    ? { type: "FeatureCollection", features }
    : null;
}

export function mapStyleForToken(token: string): string | null {
  return token ? "mapbox://styles/mapbox/streets-v12" : null;
}

export function initialMapViewport(
  bounds: MapBounds,
  fullscreen: boolean,
  maxZoom?: number,
) {
  return {
    bounds: bounds as LngLatBoundsLike,
    fitBoundsOptions: {
      padding: fullscreen ? 72 : 40,
      maxZoom: maxZoom ?? 14,
      duration: 0,
    },
  };
}

function addCardLayers(map: MapboxMap, collection: FeatureCollection) {
  const sourceId = "reply-card-geojson";
  map.addSource(sourceId, {
    type: "geojson",
    data: collection as never,
  });
  map.addLayer({
    id: "reply-card-polygons",
    type: "fill",
    source: sourceId,
    filter: ["==", ["geometry-type"], "Polygon"],
    paint: {
      "fill-color": ["coalesce", ["get", "color"], "#0891b2"],
      "fill-opacity": 0.24,
    },
  });
  map.addLayer({
    id: "reply-card-polygon-outlines",
    type: "line",
    source: sourceId,
    filter: ["==", ["geometry-type"], "Polygon"],
    paint: {
      "line-color": ["coalesce", ["get", "color"], "#0e7490"],
      "line-width": 2,
    },
  });
  map.addLayer({
    id: "reply-card-lines",
    type: "line",
    source: sourceId,
    filter: ["==", ["geometry-type"], "LineString"],
    paint: {
      "line-color": ["coalesce", ["get", "color"], "#2563eb"],
      "line-width": 4,
      "line-opacity": 0.88,
    },
  });
  map.addLayer({
    id: "reply-card-points",
    type: "circle",
    source: sourceId,
    filter: ["==", ["geometry-type"], "Point"],
    paint: {
      "circle-color": ["coalesce", ["get", "color"], "#f97316"],
      "circle-radius": 7,
      "circle-stroke-color": "#ffffff",
      "circle-stroke-width": 2,
    },
  });
  map.addLayer({
    id: "reply-card-point-labels",
    type: "symbol",
    source: sourceId,
    filter: ["==", ["geometry-type"], "Point"],
    layout: {
      "text-field": ["coalesce", ["get", "label"], ""],
      "text-size": 12,
      "text-offset": [0, 1.2],
      "text-anchor": "top",
    },
    paint: {
      "text-color": "#111827",
      "text-halo-color": "#ffffff",
      "text-halo-width": 1.5,
    },
  });
}

function MapPlaceholderBackdrop() {
  return (
    <svg
      className="web-map-card-placeholder-map"
      viewBox="0 0 720 360"
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
      data-testid="map-placeholder-background"
    >
      <defs>
        <linearGradient id="map-placeholder-land" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#edf3ec" />
          <stop offset="1" stopColor="#dfe9e2" />
        </linearGradient>
        <linearGradient id="map-placeholder-water" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#c9e4ee" />
          <stop offset="1" stopColor="#b8d9e8" />
        </linearGradient>
        <pattern
          id="map-placeholder-blocks"
          width="54"
          height="42"
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(-9)"
        >
          <rect width="54" height="42" fill="transparent" />
          <rect x="7" y="7" width="28" height="18" rx="3" fill="#d6ddd7" />
          <rect x="38" y="8" width="10" height="28" rx="2" fill="#d2dad4" />
        </pattern>
        <filter id="map-placeholder-pin-shadow" x="-60%" y="-60%" width="220%" height="220%">
          <feDropShadow dx="0" dy="2" stdDeviation="2.5" floodColor="#334155" floodOpacity=".2" />
        </filter>
      </defs>

      <rect width="720" height="360" fill="url(#map-placeholder-land)" />
      <rect width="720" height="360" fill="url(#map-placeholder-blocks)" opacity=".78" />

      <path
        d="M-20 262C58 214 117 242 166 197C216 152 197 94 248 50C280 22 323 6 375-16H-20Z"
        fill="url(#map-placeholder-water)"
      />
      <path
        d="M528 382C537 326 570 306 610 282C661 251 698 203 744 137V382Z"
        fill="url(#map-placeholder-water)"
      />
      <path
        d="M-8 254C63 214 121 234 166 191C213 146 198 94 251 47C288 15 329 3 372-15"
        fill="none"
        stroke="#f8fafc"
        strokeWidth="5"
        opacity=".92"
      />

      <g fill="#b8cfb1" opacity=".95">
        <path d="M281 44l86-20 52 38-20 61-94 4-42-38Z" />
        <path d="M449 221l74-17 39 43-18 55-91 3-30-44Z" />
        <path d="M90 284l77-21 41 35-17 50H83l-23-34Z" />
      </g>
      <g fill="none" strokeLinecap="round" strokeLinejoin="round">
        <path
          d="M-18 326C98 266 153 274 228 226C316 170 384 169 476 109C553 59 623 44 748 49"
          stroke="#ffffff"
          strokeWidth="15"
        />
        <path
          d="M-18 326C98 266 153 274 228 226C316 170 384 169 476 109C553 59 623 44 748 49"
          stroke="#e6b96f"
          strokeWidth="4"
        />
        <path
          d="M188-14C209 62 254 102 321 140C392 180 432 228 448 382"
          stroke="#ffffff"
          strokeWidth="11"
        />
        <path
          d="M188-14C209 62 254 102 321 140C392 180 432 228 448 382"
          stroke="#cdd4d8"
          strokeWidth="3"
        />
        <path
          d="M47 97C145 113 207 142 259 190C315 243 356 279 394 372"
          stroke="#ffffff"
          strokeWidth="8"
        />
        <path
          d="M47 97C145 113 207 142 259 190C315 243 356 279 394 372"
          stroke="#d5dce0"
          strokeWidth="2.5"
        />
        <path
          d="M330 12C390 71 447 94 532 113C601 129 651 155 727 212"
          stroke="#ffffff"
          strokeWidth="8"
        />
        <path
          d="M330 12C390 71 447 94 532 113C601 129 651 155 727 212"
          stroke="#d5dce0"
          strokeWidth="2.5"
        />
      </g>

      <g fill="#94a3b8" opacity=".42">
        <circle cx="106" cy="174" r="3" />
        <circle cx="169" cy="104" r="3" />
        <circle cx="301" cy="302" r="3" />
        <circle cx="521" cy="168" r="3" />
        <circle cx="628" cy="234" r="3" />
        <circle cx="658" cy="94" r="3" />
      </g>
      <g filter="url(#map-placeholder-pin-shadow)">
        <g transform="translate(147 210)">
          <path d="M0-17c-8 0-14 6-14 14 0 11 14 24 14 24S14 8 14-3c0-8-6-14-14-14Z" fill="#0f766e" />
          <circle cy="-3" r="5" fill="#f8fafc" />
        </g>
        <g transform="translate(576 110)">
          <path d="M0-17c-8 0-14 6-14 14 0 11 14 24 14 24S14 8 14-3c0-8-6-14-14-14Z" fill="#2563eb" />
          <circle cy="-3" r="5" fill="#f8fafc" />
        </g>
        <g transform="translate(506 278)">
          <path d="M0-17c-8 0-14 6-14 14 0 11 14 24 14 24S14 8 14-3c0-8-6-14-14-14Z" fill="#ea580c" />
          <circle cy="-3" r="5" fill="#f8fafc" />
        </g>
      </g>
    </svg>
  );
}

function MapCanvas({
  card,
  fullscreen = false,
  accessToken,
  configurationLoading,
  canConfigure,
  onConfigure,
}: {
  card: MapReplyCardData;
  fullscreen?: boolean;
  accessToken: string;
  configurationLoading: boolean;
  canConfigure: boolean;
  onConfigure: () => void;
}) {
  const mapElement = useRef<HTMLDivElement | null>(null);
  const mapInstance = useRef<MapboxMap | null>(null);
  const [loadState, setLoadState] = useState<MapLoadState>("loading");
  const [loadError, setLoadError] = useState("");
  const collection = useMemo(() => featureCollectionForCard(card), [card]);
  const bounds = useMemo(
    () => card.bbox ?? dataBoundsForCard(card),
    [card],
  );
  const mapStyle = mapStyleForToken(accessToken);

  useEffect(() => {
    const container = mapElement.current;
    if (!container || !collection || !bounds || !mapStyle) return;
    if (import.meta.env.MODE === "test") {
      setLoadState("ready");
      return;
    }

    let disposed = false;
    let mapLoaded = false;
    let resizeObserver: ResizeObserver | null = null;

    void import("mapbox-gl")
      .then((module) => {
        if (disposed || !mapElement.current) return;
        const mapboxgl = module.default;
        mapboxgl.accessToken = accessToken;
        const map = new mapboxgl.Map({
          container: mapElement.current,
          style: mapStyle,
          ...initialMapViewport(bounds, fullscreen, card.zoom),
          attributionControl: true,
        });
        mapInstance.current = map;
        map.addControl(
          new mapboxgl.NavigationControl({
            showCompass: false,
            visualizePitch: false,
          }),
          "top-right",
        );
        map.once("load", () => {
          if (disposed) return;
          mapLoaded = true;
          addCardLayers(map, collection);
          setLoadState("ready");
        });
        map.on("error", (event) => {
          if (disposed || mapLoaded) return;
          setLoadState("error");
          setLoadError(event.error?.message ?? "Mapbox GL failed to load");
        });
        map.on("click", "reply-card-points", (event) => {
          const feature = event.features?.[0] as unknown as {
            geometry?: { type?: unknown; coordinates?: unknown };
            properties?: Record<string, unknown>;
          } | undefined;
          const geometry = feature?.geometry;
          if (
            geometry?.type !== "Point"
            || !Array.isArray(geometry.coordinates)
            || geometry.coordinates.length < 2
            || typeof geometry.coordinates[0] !== "number"
            || typeof geometry.coordinates[1] !== "number"
          ) {
            return;
          }
          const [longitude, latitude] = geometry.coordinates;
          const properties = feature?.properties;
          const label =
            typeof properties?.label === "string"
              ? properties.label
              : card.title;
          const description =
            typeof properties?.description === "string"
              ? properties.description
              : "";
          new mapboxgl.Popup({ offset: 12 })
            .setLngLat([longitude, latitude])
            .setText(description ? `${label} — ${description}` : label)
            .addTo(map);
        });
        map.on("mouseenter", "reply-card-points", () => {
          map.getCanvas().style.cursor = "pointer";
        });
        map.on("mouseleave", "reply-card-points", () => {
          map.getCanvas().style.cursor = "";
        });
        if (typeof ResizeObserver !== "undefined") {
          resizeObserver = new ResizeObserver(() => map.resize());
          resizeObserver.observe(container);
        }
      })
      .catch((error: unknown) => {
        if (disposed) return;
        setLoadState("error");
        setLoadError(
          error instanceof Error ? error.message : "Mapbox GL failed to load",
        );
      });

    return () => {
      disposed = true;
      resizeObserver?.disconnect();
      mapInstance.current?.remove();
      mapInstance.current = null;
    };
  }, [
    bounds,
    card.title,
    card.zoom,
    collection,
    accessToken,
    fullscreen,
    mapStyle,
  ]);

  if (!bounds || !collection) {
    return (
      <div
        className="web-map-card-canvas"
        role="status"
        aria-label="Map data pending"
      >
        <MapPinned size={28} aria-hidden="true" />
        <strong>等待地图数据</strong>
        <span>模型已返回 map-card 标记，等待 Artifact 或内联几何数据。</span>
      </div>
    );
  }

  if (!mapStyle) {
    return (
      <div
        className="web-map-card-map-frame"
        data-basemap="mapbox"
        data-map-state="token-required"
      >
        <div
          className="web-map-card-mapbox"
          role="region"
          aria-label="Interactive Mapbox map"
          data-map-engine="mapbox-gl"
        />
        <MapPlaceholderBackdrop />
        <div className="web-map-card-map-state is-token-required" role="alert">
          <div className="web-map-card-token-prompt">
            <KeyRound size={24} aria-hidden="true" />
            <strong>
              {configurationLoading ? "正在读取 Mapbox 配置" : "需要公开 Mapbox Token"}
            </strong>
            <span>
              配置以 pk. 开头并限制站点来源的公开浏览器 Token 后，即可加载
              Mapbox Streets。
            </span>
            <button
              type="button"
              className="web-map-card-configure"
              onClick={onConfigure}
              disabled={!canConfigure}
            >
              {canConfigure ? "配置 Mapbox Key" : "请联系管理员配置"}
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className="web-map-card-map-frame"
      data-basemap="mapbox"
      data-map-state={loadState}
    >
      <div
        ref={mapElement}
        className="web-map-card-mapbox"
        role="region"
        aria-label="Interactive Mapbox map"
        data-map-engine="mapbox-gl"
      />
      {loadState === "loading" && (
        <div className="web-map-card-map-state" role="status">
          正在加载交互地图…
        </div>
      )}
      {loadState === "error" && (
        <div className="web-map-card-map-state is-error" role="alert">
          <strong>地图加载失败</strong>
          <span>{loadError}</span>
          <button
            type="button"
            className="web-map-card-configure"
            onClick={onConfigure}
            disabled={!canConfigure}
          >
            更新 Mapbox Key
          </button>
        </div>
      )}
    </div>
  );
}

const MapReplyCard = memo(function MapReplyCard({ card }: Props) {
  const [fullscreen, setFullscreen] = useState(false);
  const [configurationOpen, setConfigurationOpen] = useState(false);
  const mapsConfiguration = useMapsConfiguration();
  const status = card.status ?? "loading";
  const detail =
    card.summary
    ?? card.fallbackText
    ?? "地图卡片已由平台识别；第三方 Provider 可通过 map-card 标记和 MCP 地理工具提供数据。";
  const openConfiguration = () => {
    setConfigurationOpen(true);
  };
  const body = (fullscreenBody = false) => (
    <div
      className={`web-map-card is-${status}`}
      role="group"
      aria-label={`Map card: ${card.title}`}
    >
      <div className="web-map-card-header">
        <div className="web-map-card-title">
          <MapPinned size={16} aria-hidden="true" />
          <span>{card.title}</span>
        </div>
        <span className="web-map-card-status">{statusLabel(status)}</span>
        {!fullscreenBody && (
          <button
            type="button"
            className="web-map-card-fullscreen"
            onClick={() => setFullscreen(true)}
            aria-label="Open map card fullscreen"
          >
            <Expand size={16} aria-hidden="true" />
            <span>全屏</span>
          </button>
        )}
      </div>
      <MapCanvas
        card={card}
        fullscreen={fullscreenBody}
        accessToken={mapsConfiguration.mapboxAccessToken ?? ""}
        configurationLoading={mapsConfiguration.loading}
        canConfigure={mapsConfiguration.canConfigure}
        onConfigure={openConfiguration}
      />
      <div className="web-map-card-body">
        <p>{detail}</p>
        <dl>
          {card.intent && (
            <>
              <dt>Intent</dt>
              <dd>{card.intent}</dd>
            </>
          )}
          {card.inputRef && (
            <>
              <dt>Input ref</dt>
              <dd>{card.inputRef}</dd>
            </>
          )}
          {card.artifactId && (
            <>
              <dt>Artifact</dt>
              <dd>{card.artifactId}</dd>
            </>
          )}
          {card.points?.length ? (
            <>
              <dt>Points</dt>
              <dd>{card.points.length}</dd>
            </>
          ) : null}
          {card.lines?.length ? (
            <>
              <dt>Lines</dt>
              <dd>{card.lines.length}</dd>
            </>
          ) : null}
          {card.polygons?.length ? (
            <>
              <dt>Polygons</dt>
              <dd>{card.polygons.length}</dd>
            </>
          ) : null}
        </dl>
      </div>
    </div>
  );
  return (
    <>
      {body(false)}
      {fullscreen && (
        <div
          className="web-map-card-modal"
          role="dialog"
          aria-modal="true"
          aria-label={`Fullscreen map card: ${card.title}`}
        >
          <div
            className="web-map-card-modal-backdrop"
            onClick={() => setFullscreen(false)}
          />
          <div className="web-map-card-modal-panel">
            <button
              type="button"
              className="web-map-card-modal-close"
              onClick={() => setFullscreen(false)}
            >
              Close
            </button>
            {body(true)}
          </div>
        </div>
      )}
      {configurationOpen ? (
        <MapsConfigurationModal
          initialProvider="mapbox"
          onClose={() => setConfigurationOpen(false)}
          onSaved={() => setConfigurationOpen(false)}
        />
      ) : null}
    </>
  );
}, (previous, next) => sameMapReplyCard(previous.card, next.card));

export default MapReplyCard;

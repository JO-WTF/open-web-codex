import { memo, useEffect, useMemo, useRef, useState } from "react";
import Expand from "lucide-react/dist/esm/icons/expand";
import KeyRound from "lucide-react/dist/esm/icons/key-round";
import MapPinned from "lucide-react/dist/esm/icons/map-pinned";
import type { Map as MapboxMap } from "mapbox-gl";
import "mapbox-gl/dist/mapbox-gl.css";
import { platformClient } from "../../../../browser/session";
import type {
  GeoJson,
  MapBounds,
  MapLayer,
  MapReplyCard as MapReplyCardData,
} from "../../../utils/replyCards";
import { useMapsConfiguration } from "../../../services/mapsConfiguration";
import MapsConfigurationModal from "./MapsConfigurationModal";

type Props = {
  card: MapReplyCardData;
};

type LoadedSource = {
  id: string;
  data: GeoJson;
};

type MapLoadState = "loading" | "ready" | "error";

export function sameMapReplyCard(
  left: MapReplyCardData,
  right: MapReplyCardData,
): boolean {
  return left === right || JSON.stringify(left) === JSON.stringify(right);
}

function statusLabel(status: MapReplyCardData["status"]) {
  if (status === "ready") return "Ready";
  if (status === "error") return "Failed";
  return "Loading data";
}

function extendBounds(
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

function collectCoordinates(
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
    for (const entry of value) collectCoordinates(entry, visit);
    return;
  }
  if (!value || typeof value !== "object") return;
  const record = value as Record<string, unknown>;
  if (record.type === "FeatureCollection") collectCoordinates(record.features, visit);
  else if (record.type === "Feature") collectCoordinates(record.geometry, visit);
  else if (record.type === "GeometryCollection") collectCoordinates(record.geometries, visit);
  else collectCoordinates(record.coordinates, visit);
}

export function dataBoundsForSources(sources: LoadedSource[]): MapBounds | null {
  let bounds: MapBounds | null = null;
  for (const source of sources) {
    collectCoordinates(source.data, (longitude, latitude) => {
      if (
        longitude >= -180
        && longitude <= 180
        && latitude >= -90
        && latitude <= 90
      ) {
        bounds = extendBounds(bounds, longitude, latitude);
      }
    });
  }
  if (!bounds) return null;
  if (bounds[0] === bounds[2] && bounds[1] === bounds[3]) {
    return [
      bounds[0] - 0.08,
      bounds[1] - 0.08,
      bounds[2] + 0.08,
      bounds[3] + 0.08,
    ];
  }
  return bounds;
}

export function mapStyleForToken(token: string): string | null {
  return token ? "mapbox://styles/mapbox/streets-v12" : null;
}

function fitOptions(card: MapReplyCardData, fullscreen: boolean) {
  if (card.viewport.mode !== "fit") return undefined;
  return {
    padding: card.viewport.padding ?? (fullscreen ? 72 : 40),
    maxZoom: card.viewport.maxZoom ?? 14,
    duration: 0,
  };
}

function geometryFilter(geometry: MapLayer["geometry"]) {
  if (geometry === "point") {
    return ["in", ["geometry-type"], ["literal", ["Point", "MultiPoint"]]];
  }
  if (geometry === "line") {
    return ["in", ["geometry-type"], ["literal", ["LineString", "MultiLineString"]]];
  }
  return ["in", ["geometry-type"], ["literal", ["Polygon", "MultiPolygon"]]];
}

function addLayer(map: MapboxMap, layer: MapLayer) {
  const source = `reply-source-${layer.source}`;
  const filter = geometryFilter(layer.geometry);
  if (layer.geometry === "point") {
    map.addLayer({
      id: `reply-layer-${layer.id}`,
      type: "circle",
      source,
      filter,
      paint: {
        "circle-color": layer.style.color ?? "#f97316",
        "circle-opacity": layer.style.opacity ?? 1,
        "circle-radius": layer.style.radius ?? 7,
        "circle-stroke-color": layer.style.strokeColor ?? "#ffffff",
        "circle-stroke-width": layer.style.strokeWidth ?? 2,
        "circle-stroke-opacity": layer.style.strokeOpacity ?? 1,
      },
    } as never);
  } else if (layer.geometry === "line") {
    map.addLayer({
      id: `reply-layer-${layer.id}`,
      type: "line",
      source,
      filter,
      layout: {
        "line-cap": layer.style.cap ?? "round",
        "line-join": layer.style.join ?? "round",
      },
      paint: {
        "line-color": layer.style.color ?? "#2563eb",
        "line-opacity": layer.style.opacity ?? 0.9,
        "line-width": layer.style.width ?? 4,
        ...(layer.style.dash ? { "line-dasharray": layer.style.dash } : {}),
      },
    } as never);
  } else {
    map.addLayer({
      id: `reply-layer-${layer.id}`,
      type: "fill",
      source,
      filter,
      paint: {
        "fill-color": layer.style.fillColor ?? "#0891b2",
        "fill-opacity": layer.style.fillOpacity ?? 0.24,
      },
    } as never);
    if ((layer.style.strokeWidth ?? 2) > 0) {
      map.addLayer({
        id: `reply-layer-${layer.id}-stroke`,
        type: "line",
        source,
        filter,
        paint: {
          "line-color": layer.style.strokeColor ?? "#0e7490",
          "line-width": layer.style.strokeWidth ?? 2,
          "line-opacity": layer.style.strokeOpacity ?? 1,
          ...(layer.style.strokeDash
            ? { "line-dasharray": layer.style.strokeDash }
            : {}),
        },
      } as never);
    }
  }
  if (layer.labelProperty) {
    map.addLayer({
      id: `reply-layer-${layer.id}-labels`,
      type: "symbol",
      source,
      filter,
      layout: {
        "text-field": ["coalesce", ["get", layer.labelProperty], ""],
        "text-size": 12,
        "text-offset": layer.geometry === "point" ? [0, 1.2] : [0, 0],
        "text-anchor": layer.geometry === "point" ? "top" : "center",
      },
      paint: {
        "text-color": "#111827",
        "text-halo-color": "#ffffff",
        "text-halo-width": 1.5,
      },
    } as never);
  }
}

function useLoadedSources(card: MapReplyCardData) {
  const [sources, setSources] = useState<LoadedSource[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    setSources([]);
    setError("");
    void Promise.all(card.sources.map(async (source): Promise<LoadedSource> => {
      if (source.data.type === "inline") {
        return { id: source.id, data: source.data.geojson };
      }
      const data = await platformClient.readReplyArtifact(source.data.url);
      if (!data || typeof data.type !== "string") {
        throw new Error("Reply Artifact did not contain GeoJSON.");
      }
      return { id: source.id, data: data as GeoJson };
    }))
      .then((loaded) => {
        if (!disposed) setSources(loaded);
      })
      .catch((reason: unknown) => {
        if (!disposed) {
          setError(reason instanceof Error ? reason.message : "Map data failed to load.");
        }
      });
    return () => {
      disposed = true;
    };
  }, [card.sources]);
  return { sources, error };
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
  const loaded = useLoadedSources(card);
  const bounds = useMemo(() => dataBoundsForSources(loaded.sources), [loaded.sources]);
  const mapStyle = mapStyleForToken(accessToken);

  useEffect(() => {
    const container = mapElement.current;
    if (!container || !loaded.sources.length || !mapStyle || loaded.error) return;
    if (card.viewport.mode === "fit" && !bounds) {
      setLoadState("error");
      setLoadError("GeoJSON does not contain valid coordinates.");
      return;
    }
    if (import.meta.env.MODE === "test") {
      setLoadState("ready");
      return;
    }

    let disposed = false;
    let resizeObserver: ResizeObserver | null = null;
    let fittedAfterLayout = false;
    const applyViewport = (map: MapboxMap) => {
      map.resize();
      if (card.viewport.mode === "camera") {
        map.jumpTo({
          center: card.viewport.center,
          zoom: card.viewport.zoom,
          bearing: card.viewport.bearing ?? 0,
          pitch: card.viewport.pitch ?? 0,
        });
      } else if (bounds) {
        map.fitBounds(bounds, fitOptions(card, fullscreen));
        if (
          card.viewport.minZoom != null
          && map.getZoom() < card.viewport.minZoom
        ) {
          map.setZoom(card.viewport.minZoom);
        }
      }
    };

    void import("mapbox-gl")
      .then((module) => {
        if (disposed || !mapElement.current) return;
        const mapboxgl = module.default;
        mapboxgl.accessToken = accessToken;
        const camera = card.viewport.mode === "camera"
          ? {
            center: card.viewport.center,
            zoom: card.viewport.zoom,
            bearing: card.viewport.bearing ?? 0,
            pitch: card.viewport.pitch ?? 0,
          }
          : { center: [0, 0] as [number, number], zoom: 0 };
        const map = new mapboxgl.Map({
          container: mapElement.current,
          style: mapStyle,
          ...camera,
          attributionControl: true,
        });
        mapInstance.current = map;
        map.addControl(new mapboxgl.NavigationControl(), "top-right");
        map.once("load", () => {
          if (disposed) return;
          for (const source of loaded.sources) {
            map.addSource(`reply-source-${source.id}`, {
              type: "geojson",
              data: source.data as never,
            });
          }
          for (const layer of card.layers) addLayer(map, layer);
          applyViewport(map);
          setLoadState("ready");
        });
        map.on("error", (event) => {
          if (disposed) return;
          setLoadState("error");
          setLoadError(event.error?.message ?? "Mapbox GL failed to load.");
        });
        if (typeof ResizeObserver !== "undefined") {
          resizeObserver = new ResizeObserver(() => {
            map.resize();
            if (!fittedAfterLayout && container.clientWidth > 0 && container.clientHeight > 0) {
              fittedAfterLayout = true;
              applyViewport(map);
            }
          });
          resizeObserver.observe(container);
        }
      })
      .catch((reason: unknown) => {
        if (disposed) return;
        setLoadState("error");
        setLoadError(reason instanceof Error ? reason.message : "Mapbox GL failed to load.");
      });

    return () => {
      disposed = true;
      resizeObserver?.disconnect();
      mapInstance.current?.remove();
      mapInstance.current = null;
    };
  }, [accessToken, bounds, card, fullscreen, loaded.error, loaded.sources, mapStyle]);

  if (loaded.error) {
    return (
      <div className="web-map-card-canvas" role="alert">
        <MapPinned size={28} aria-hidden="true" />
        <strong>地图数据加载失败</strong>
        <span>{loaded.error}</span>
      </div>
    );
  }
  if (!loaded.sources.length) {
    return (
      <div className="web-map-card-canvas" role="status" aria-label="Map data loading">
        <MapPinned size={28} aria-hidden="true" />
        <strong>正在读取地图数据</strong>
      </div>
    );
  }
  if (!mapStyle) {
    return (
      <div className="web-map-card-map-frame" data-map-state="token-required">
        <div className="web-map-card-map-state is-token-required" role="alert">
          <div className="web-map-card-token-prompt">
            <KeyRound size={24} aria-hidden="true" />
            <strong>
              {configurationLoading ? "正在读取 Mapbox 配置" : "需要公开 Mapbox Token"}
            </strong>
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
    <div className="web-map-card-map-frame" data-map-state={loadState}>
      <div
        ref={mapElement}
        className="web-map-card-mapbox"
        role="region"
        aria-label="Interactive Mapbox map"
      />
      {loadState === "loading" ? (
        <div className="web-map-card-map-state" role="status">正在加载交互地图…</div>
      ) : null}
      {loadState === "error" ? (
        <div className="web-map-card-map-state is-error" role="alert">
          <strong>地图加载失败</strong>
          <span>{loadError}</span>
        </div>
      ) : null}
    </div>
  );
}

const MapReplyCard = memo(function MapReplyCard({ card }: Props) {
  const [fullscreen, setFullscreen] = useState(false);
  const [configurationOpen, setConfigurationOpen] = useState(false);
  const mapsConfiguration = useMapsConfiguration();
  const detail = card.summary ?? card.fallbackText ?? "地图数据已就绪。";
  const body = (fullscreenBody = false) => (
    <div
      className={`web-map-card is-${card.status}`}
      role="group"
      aria-label={`Map card: ${card.title}`}
    >
      <div className="web-map-card-header">
        <div className="web-map-card-title">
          <MapPinned size={16} aria-hidden="true" />
          <span>{card.title}</span>
        </div>
        <span className="web-map-card-status">{statusLabel(card.status)}</span>
        {!fullscreenBody ? (
          <button
            type="button"
            className="web-map-card-fullscreen"
            onClick={() => setFullscreen(true)}
            aria-label="Open map card fullscreen"
          >
            <Expand size={16} aria-hidden="true" />
            <span>全屏</span>
          </button>
        ) : null}
      </div>
      <MapCanvas
        card={card}
        fullscreen={fullscreenBody}
        accessToken={mapsConfiguration.mapboxAccessToken ?? ""}
        configurationLoading={mapsConfiguration.loading}
        canConfigure={mapsConfiguration.canConfigure}
        onConfigure={() => setConfigurationOpen(true)}
      />
      <div className="web-map-card-body">
        <p>{detail}</p>
        <dl>
          <dt>Sources</dt>
          <dd>{card.sources.length}</dd>
          <dt>Layers</dt>
          <dd>{card.layers.length}</dd>
          <dt>Viewport</dt>
          <dd>{card.viewport.mode === "fit" ? "Fit data" : `Zoom ${card.viewport.zoom}`}</dd>
        </dl>
        {card.legend ? (
          <div className="web-map-card-legend" aria-label={card.legend.title ?? "Map legend"}>
            {card.legend.title ? <strong>{card.legend.title}</strong> : null}
            {card.legend.items.map((item) => (
              <span key={`${item.label}-${item.color}`}>
                <i style={{ backgroundColor: item.color }} />
                {item.label}
              </span>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
  return (
    <>
      {body(false)}
      {fullscreen ? (
        <div
          className="web-map-card-modal"
          role="dialog"
          aria-modal="true"
          aria-label={`Fullscreen map card: ${card.title}`}
        >
          <div className="web-map-card-modal-backdrop" onClick={() => setFullscreen(false)} />
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
      ) : null}
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

export type MapBounds = [number, number, number, number];

export type MapViewport =
  | {
    mode: "fit";
    padding?: number | { top: number; right: number; bottom: number; left: number };
    maxZoom?: number;
    minZoom?: number;
  }
  | {
    mode: "camera";
    center: [number, number];
    zoom: number;
    bearing?: number;
    pitch?: number;
  };

export type MapSourceData =
  | { type: "inline"; format: "geojson"; geojson: GeoJson }
  | {
    type: "artifact";
    format: "geojson";
    artifactId: string;
    mimeType?: string;
    url: string;
  };

export type MapSource = {
  id: string;
  data: MapSourceData;
};

export type PointLayerStyle = {
  color?: string;
  opacity?: number;
  radius?: number;
  size?: number;
  shape?: "circle" | "square" | "diamond" | "triangle" | "pin";
  icon?: {
    url: string;
    scale?: number;
    anchor?:
      | "center"
      | "top"
      | "bottom"
      | "left"
      | "right"
      | "top-left"
      | "top-right"
      | "bottom-left"
      | "bottom-right";
    rotation?: number;
    allowOverlap?: boolean;
  };
  strokeColor?: string;
  strokeWidth?: number;
  strokeOpacity?: number;
};

export type LineLayerStyle = {
  color?: string;
  opacity?: number;
  width?: number;
  dash?: number[];
  cap?: "butt" | "round" | "square";
  join?: "bevel" | "round" | "miter";
};

export type PolygonLayerStyle = {
  fillColor?: string;
  fillOpacity?: number;
  strokeColor?: string;
  strokeWidth?: number;
  strokeOpacity?: number;
  strokeDash?: number[];
};

type MapLayerBase = {
  id: string;
  source: string;
  labelProperty?: string;
  hover?: {
    titleProperty?: string;
    fields: Array<{ property: string; label?: string }>;
  };
};

export type MapLayer =
  | (MapLayerBase & { geometry: "point"; style: PointLayerStyle })
  | (MapLayerBase & { geometry: "line"; style: LineLayerStyle })
  | (MapLayerBase & { geometry: "polygon"; style: PolygonLayerStyle });

export type MapReplyCard = {
  type: "card";
  kind: "map.v2";
  id: string;
  title: string;
  intent: string;
  fallbackText?: string;
  summary?: string;
  status: "loading" | "ready" | "error";
  viewport: MapViewport;
  sources: MapSource[];
  layers: MapLayer[];
  legend?: {
    title?: string;
    items: Array<{ label: string; color: string }>;
  };
};

export type ReplyCard = MapReplyCard;
export type InlineVisualizationArtifact = {
  ref: string;
  rendererKind: "map.v2";
  card: MapReplyCard;
};

export type GeoJson = Record<string, unknown> & { type: string };

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object" && !Array.isArray(value));
}

function nonemptyString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function finiteNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function geoJson(value: unknown): GeoJson | undefined {
  if (!isRecord(value) || !nonemptyString(value.type)) return undefined;
  return value as GeoJson;
}

function viewport(value: unknown): MapViewport | undefined {
  if (!isRecord(value)) return undefined;
  if (value.mode === "camera") {
    if (!Array.isArray(value.center) || value.center.length !== 2) return undefined;
    const longitude = finiteNumber(value.center[0]);
    const latitude = finiteNumber(value.center[1]);
    const zoom = finiteNumber(value.zoom);
    if (longitude == null || latitude == null || zoom == null) return undefined;
    return {
      mode: "camera",
      center: [longitude, latitude],
      zoom,
      bearing: finiteNumber(value.bearing),
      pitch: finiteNumber(value.pitch),
    };
  }
  if (value.mode !== "fit") return undefined;
  let padding: Extract<MapViewport, { mode: "fit" }>["padding"];
  if (finiteNumber(value.padding) != null) {
    padding = finiteNumber(value.padding);
  } else if (isRecord(value.padding)) {
    const top = finiteNumber(value.padding.top);
    const right = finiteNumber(value.padding.right);
    const bottom = finiteNumber(value.padding.bottom);
    const left = finiteNumber(value.padding.left);
    if (top == null || right == null || bottom == null || left == null) return undefined;
    padding = { top, right, bottom, left };
  }
  return {
    mode: "fit",
    padding,
    maxZoom: finiteNumber(value.max_zoom),
    minZoom: finiteNumber(value.min_zoom),
  };
}

function source(value: unknown): MapSource | undefined {
  if (!isRecord(value) || !isRecord(value.data)) return undefined;
  const id = nonemptyString(value.id);
  if (!id || value.data.format !== "geojson") return undefined;
  if (value.data.type === "inline") {
    const data = geoJson(value.data.geojson);
    return data
      ? { id, data: { type: "inline", format: "geojson", geojson: data } }
      : undefined;
  }
  if (value.data.type !== "artifact") return undefined;
  const artifactId = nonemptyString(value.data.artifact_id);
  const url = nonemptyString(value.data.url);
  if (!artifactId || !url || !url.startsWith("/api/runs/")) return undefined;
  return {
    id,
    data: {
      type: "artifact",
      format: "geojson",
      artifactId,
      mimeType: nonemptyString(value.data.mime_type),
      url,
    },
  };
}

function commonLayer(value: Record<string, unknown>) {
  const id = nonemptyString(value.id);
  const sourceId = nonemptyString(value.source);
  if (!id || !sourceId || !isRecord(value.style)) return undefined;
  return {
    id,
    source: sourceId,
    labelProperty: nonemptyString(value.label_property),
    hover: layerHover(value.hover),
  };
}

function numberList(value: unknown): number[] | undefined {
  if (!Array.isArray(value) || value.some((entry) => finiteNumber(entry) == null)) return undefined;
  return value as number[];
}

function supportedMapIconUrl(value: unknown): string | undefined {
  const url = nonemptyString(value);
  if (!url || url.length > 2048) return undefined;
  try {
    const parsed = new URL(url);
    return parsed.protocol === "https:"
      && /\.(?:png|jpe?g|webp)$/i.test(parsed.pathname)
      ? url
      : undefined;
  } catch {
    return undefined;
  }
}

function pointIcon(value: unknown): PointLayerStyle["icon"] {
  if (!isRecord(value)) return undefined;
  const url = supportedMapIconUrl(value.url);
  if (!url) return undefined;
  const anchor = value.anchor;
  const supportedAnchors = new Set([
    "center",
    "top",
    "bottom",
    "left",
    "right",
    "top-left",
    "top-right",
    "bottom-left",
    "bottom-right",
  ]);
  return {
    url,
    scale: finiteNumber(value.scale),
    anchor: typeof anchor === "string" && supportedAnchors.has(anchor)
      ? anchor as NonNullable<PointLayerStyle["icon"]>["anchor"]
      : undefined,
    rotation: finiteNumber(value.rotation),
    allowOverlap: typeof value.allow_overlap === "boolean"
      ? value.allow_overlap
      : undefined,
  };
}

function layerHover(value: unknown): MapLayerBase["hover"] {
  if (!isRecord(value)) return undefined;
  const titleProperty = nonemptyString(value.title_property);
  if (!Array.isArray(value.fields)) {
    return titleProperty ? { titleProperty, fields: [] } : undefined;
  }
  const fields = value.fields.flatMap((field) => {
    if (!isRecord(field)) return [];
    const property = nonemptyString(field.property);
    if (!property) return [];
    return [{ property, label: nonemptyString(field.label) }];
  });
  return titleProperty || fields.length ? { titleProperty, fields } : undefined;
}

function layer(value: unknown): MapLayer | undefined {
  if (!isRecord(value)) return undefined;
  const common = commonLayer(value);
  if (!common) return undefined;
  const style = value.style as Record<string, unknown>;
  if (value.geometry === "point") {
    const shape = style.shape;
    return {
      ...common,
      geometry: "point",
      style: {
        color: nonemptyString(style.color),
        opacity: finiteNumber(style.opacity),
        radius: finiteNumber(style.radius),
        size: finiteNumber(style.size),
        shape: shape === "circle"
          || shape === "square"
          || shape === "diamond"
          || shape === "triangle"
          || shape === "pin"
          ? shape
          : undefined,
        icon: pointIcon(style.icon),
        strokeColor: nonemptyString(style.stroke_color),
        strokeWidth: finiteNumber(style.stroke_width),
        strokeOpacity: finiteNumber(style.stroke_opacity),
      },
    };
  }
  if (value.geometry === "line") {
    const cap = style.cap;
    const join = style.join;
    return {
      ...common,
      geometry: "line",
      style: {
        color: nonemptyString(style.color),
        opacity: finiteNumber(style.opacity),
        width: finiteNumber(style.width),
        dash: numberList(style.dash),
        cap: cap === "butt" || cap === "round" || cap === "square" ? cap : undefined,
        join: join === "bevel" || join === "round" || join === "miter" ? join : undefined,
      },
    };
  }
  if (value.geometry !== "polygon") return undefined;
  return {
    ...common,
    geometry: "polygon",
    style: {
      fillColor: nonemptyString(style.fill_color),
      fillOpacity: finiteNumber(style.fill_opacity),
      strokeColor: nonemptyString(style.stroke_color),
      strokeWidth: finiteNumber(style.stroke_width),
      strokeOpacity: finiteNumber(style.stroke_opacity),
      strokeDash: numberList(style.stroke_dash),
    },
  };
}

function legend(value: unknown): MapReplyCard["legend"] {
  if (!isRecord(value) || !Array.isArray(value.items)) return undefined;
  const items = value.items.flatMap((item) => {
    if (!isRecord(item)) return [];
    const label = nonemptyString(item.label);
    const color = nonemptyString(item.color);
    return label && color ? [{ label, color }] : [];
  });
  if (!items.length) return undefined;
  return { title: nonemptyString(value.title), items };
}

function parseMapRendererPayload(
  value: unknown,
  artifactRef: string,
): MapReplyCard | null {
  if (
    !isRecord(value)
  ) {
    return null;
  }
  const title = nonemptyString(value.title);
  const intent = nonemptyString(value.intent);
  const status = value.status;
  const normalizedViewport = viewport(value.viewport);
  if (
    !title
    || !intent
    || !normalizedViewport
    || !Array.isArray(value.sources)
    || !Array.isArray(value.layers)
    || !["loading", "ready", "error"].includes(String(status))
  ) {
    return null;
  }
  const sources = value.sources.map(source);
  const layers = value.layers.map(layer);
  if (sources.some((entry) => !entry) || layers.some((entry) => !entry)) return null;
  const sourceIds = new Set((sources as MapSource[]).map((entry) => entry.id));
  if ((layers as MapLayer[]).some((entry) => !sourceIds.has(entry.source))) return null;
  return {
    type: "card",
    kind: "map.v2",
    id: artifactRef,
    title,
    intent,
    fallbackText: nonemptyString(value.fallback_text),
    summary: nonemptyString(value.summary),
    status: status as MapReplyCard["status"],
    viewport: normalizedViewport,
    sources: sources as MapSource[],
    layers: layers as MapLayer[],
    legend: legend(value.legend),
  };
}

export function parseInlineVisualizationArtifact(
  value: unknown,
): InlineVisualizationArtifact | null {
  if (!isRecord(value) || !isRecord(value.renderer)) return null;
  const ref = nonemptyString(value.ref);
  if (
    !ref
    || !/^[A-Za-z0-9_.-]{1,128}$/.test(ref)
    || value.renderer.kind !== "map.v2"
  ) {
    return null;
  }
  const card = parseMapRendererPayload(value.renderer.payload, ref);
  return card ? { ref, rendererKind: "map.v2", card } : null;
}

export type ReplyCardTextPart = {
  type: "text";
  content: string;
};

export type MapPoint = {
  id?: string;
  latitude: number;
  longitude: number;
  label?: string;
  description?: string;
  color?: string;
};

export type MapLine = {
  id?: string;
  label?: string;
  color?: string;
  coordinates: [number, number][];
};

export type MapPolygon = {
  id?: string;
  label?: string;
  color?: string;
  coordinates: [number, number][][];
};

export type MapBounds = [number, number, number, number];

export type MapReplyCard = {
  type: "card";
  kind: "map.v1";
  id: string;
  title: string;
  intent?: string;
  inputRef?: string;
  artifactId?: string;
  fallbackText?: string;
  summary?: string;
  status?: "loading" | "ready" | "error";
  center?: { latitude: number; longitude: number };
  zoom?: number;
  bbox?: MapBounds;
  points?: MapPoint[];
  lines?: MapLine[];
  polygons?: MapPolygon[];
  geojson?: unknown;
};

export type ReplyCardPart = ReplyCardTextPart | MapReplyCard;

type RawCardPayload = {
  title?: unknown;
  intent?: unknown;
  input_ref?: unknown;
  inputRef?: unknown;
  artifact_id?: unknown;
  artifactId?: unknown;
  fallback_text?: unknown;
  fallbackText?: unknown;
  summary?: unknown;
  status?: unknown;
  center?: unknown;
  zoom?: unknown;
  bbox?: unknown;
  bounds?: unknown;
  points?: unknown;
  lines?: unknown;
  polygons?: unknown;
  geojson?: unknown;
};

const CARD_FENCE_RE = /(^|\n)(?<fence>`{3,}|~{3,})[ \t]*(?<tag>open-web-card\s+map\.v1|widget)[ \t]*\r?\n(?<body>[\s\S]*?)\r?\n?[ \t]*\k<fence>[ \t]*(?=\n|$)/g;
const MAX_CARD_MARKER_BYTES = 16 * 1024;

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function asNumber(value: unknown): number | undefined {
  const parsed = typeof value === "number" ? value : typeof value === "string" ? Number(value) : Number.NaN;
  return Number.isFinite(parsed) ? parsed : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object" && !Array.isArray(value));
}

function stableCardId(payload: RawCardPayload, index: number): string {
  const explicit = asString(payload.artifact_id) ?? asString(payload.artifactId) ?? asString(payload.input_ref) ?? asString(payload.inputRef);
  return explicit ? `map-${explicit.replace(/[^a-zA-Z0-9_-]/g, "-").slice(0, 64)}` : `map-card-${index}`;
}

function normalizeStatus(value: unknown, hasInlineData: boolean): MapReplyCard["status"] {
  if (value === "loading" || value === "ready" || value === "error") return value;
  return hasInlineData ? "ready" : "loading";
}

function normalizeCenter(value: unknown): MapReplyCard["center"] {
  if (!isRecord(value)) return undefined;
  const latitude = asNumber(value.latitude ?? value.lat);
  const longitude = asNumber(value.longitude ?? value.lng ?? value.lon);
  if (latitude == null || longitude == null || latitude < -90 || latitude > 90 || longitude < -180 || longitude > 180) return undefined;
  return { latitude, longitude };
}

function normalizeBounds(value: unknown): MapBounds | undefined {
  if (!Array.isArray(value) || value.length < 4) return undefined;
  const west = asNumber(value[0]);
  const south = asNumber(value[1]);
  const east = asNumber(value[2]);
  const north = asNumber(value[3]);
  if (west == null || south == null || east == null || north == null) return undefined;
  if (west < -180 || west > 180 || east < -180 || east > 180 || south < -90 || south > 90 || north < -90 || north > 90) return undefined;
  if (west > east || south > north) return undefined;
  return [west, south, east, north];
}

function normalizeCoordinate(value: unknown): [number, number] | undefined {
  if (Array.isArray(value) && value.length >= 2) {
    const longitude = asNumber(value[0]);
    const latitude = asNumber(value[1]);
    if (latitude != null && longitude != null && latitude >= -90 && latitude <= 90 && longitude >= -180 && longitude <= 180) return [longitude, latitude];
  }
  if (isRecord(value)) {
    const latitude = asNumber(value.latitude ?? value.lat);
    const longitude = asNumber(value.longitude ?? value.lng ?? value.lon);
    if (latitude != null && longitude != null && latitude >= -90 && latitude <= 90 && longitude >= -180 && longitude <= 180) return [longitude, latitude];
  }
  return undefined;
}

function normalizePoints(value: unknown): MapPoint[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const points = value.flatMap((entry, index): MapPoint[] => {
    if (!isRecord(entry)) return [];
    const latitude = asNumber(entry.latitude ?? entry.lat);
    const longitude = asNumber(entry.longitude ?? entry.lng ?? entry.lon);
    if (latitude == null || longitude == null || latitude < -90 || latitude > 90 || longitude < -180 || longitude > 180) return [];
    return [{
      id: asString(entry.id) ?? `point-${index + 1}`,
      latitude,
      longitude,
      label: asString(entry.label ?? entry.name),
      description: asString(entry.description ?? entry.address),
      color: asString(entry.color),
    }];
  });
  return points.length ? points : undefined;
}

function normalizeLines(value: unknown): MapLine[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const lines = value.flatMap((entry, index): MapLine[] => {
    if (!isRecord(entry)) return [];
    const rawCoordinates = entry.coordinates ?? entry.path;
    if (!Array.isArray(rawCoordinates)) return [];
    const coordinates = rawCoordinates.flatMap((coord) => {
      const normalized = normalizeCoordinate(coord);
      return normalized ? [normalized] : [];
    });
    if (coordinates.length < 2) return [];
    return [{
      id: asString(entry.id) ?? `line-${index + 1}`,
      label: asString(entry.label ?? entry.name),
      color: asString(entry.color),
      coordinates,
    }];
  });
  return lines.length ? lines : undefined;
}

function normalizePolygons(value: unknown): MapPolygon[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const polygons = value.flatMap((entry, index): MapPolygon[] => {
    if (!isRecord(entry) || !Array.isArray(entry.coordinates)) return [];
    const rings = entry.coordinates.flatMap((ring) => {
      if (!Array.isArray(ring)) return [];
      const coordinates = ring.flatMap((coord) => {
        const normalized = normalizeCoordinate(coord);
        return normalized ? [normalized] : [];
      });
      return coordinates.length >= 4 ? [coordinates] : [];
    });
    if (!rings.length) return [];
    return [{
      id: asString(entry.id) ?? `polygon-${index + 1}`,
      label: asString(entry.label ?? entry.name),
      color: asString(entry.color),
      coordinates: rings,
    }];
  });
  return polygons.length ? polygons : undefined;
}

function normalizePayload(payload: RawCardPayload, index: number): MapReplyCard {
  const points = normalizePoints(payload.points);
  const lines = normalizeLines(payload.lines);
  const polygons = normalizePolygons(payload.polygons);
  const hasInlineData = Boolean(payload.geojson || points?.length || lines?.length || polygons?.length);
  return {
    type: "card",
    kind: "map.v1",
    id: stableCardId(payload, index),
    title: asString(payload.title) ?? "地图卡片",
    intent: asString(payload.intent),
    inputRef: asString(payload.input_ref) ?? asString(payload.inputRef),
    artifactId: asString(payload.artifact_id) ?? asString(payload.artifactId),
    fallbackText: asString(payload.fallback_text) ?? asString(payload.fallbackText),
    summary: asString(payload.summary),
    status: normalizeStatus(payload.status, hasInlineData),
    center: normalizeCenter(payload.center),
    zoom: asNumber(payload.zoom),
    bbox: normalizeBounds(payload.bbox ?? payload.bounds),
    points,
    lines,
    polygons,
    geojson: payload.geojson,
  };
}

function parseOpenWebMapCard(body: string, index: number): MapReplyCard | null {
  if (new TextEncoder().encode(body).length > MAX_CARD_MARKER_BYTES) return null;
  try {
    const parsed = JSON.parse(body) as unknown;
    if (!isRecord(parsed)) return null;
    return normalizePayload(parsed as RawCardPayload, index);
  } catch {
    return null;
  }
}

function parseLegacyWidgetMapCard(body: string, index: number): MapReplyCard | null {
  if (new TextEncoder().encode(body).length > MAX_CARD_MARKER_BYTES) return null;
  try {
    const parsed = JSON.parse(body) as unknown;
    if (!isRecord(parsed) || parsed.widget_type !== "map") return null;
    const props = isRecord(parsed.props) ? propsWithLegacyAliases(parsed.props) : {};
    const card = normalizePayload(props, index);
    card.id = asString(parsed.id) ?? `legacy-map-card-${index}`;
    if (props.use_stored_card === true && !card.summary) card.summary = "地图数据已存储在服务端，等待平台 Artifact hydration。";
    if (!card.points && !card.lines && !card.polygons && !card.geojson) card.status = "loading";
    return card;
  } catch {
    return null;
  }
}

function propsWithLegacyAliases(props: Record<string, unknown>): RawCardPayload & { use_stored_card?: unknown } {
  return {
    ...props,
    title: props.title,
    input_ref: props.input_ref,
    artifact_id: props.artifact_id,
  };
}

export function parseReplyCards(markdown: string): ReplyCardPart[] {
  const parts: ReplyCardPart[] = [];
  let cursor = 0;
  let cardIndex = 0;
  for (const match of markdown.matchAll(CARD_FENCE_RE)) {
    const groups = match.groups;
    if (!groups) continue;
    const matchStart = match.index ?? 0;
    const leadingNewline = groups[1] ?? "";
    const fenceStart = matchStart + leadingNewline.length;
    const textBefore = markdown.slice(cursor, fenceStart);
    const card = groups.tag.startsWith("open-web-card")
      ? parseOpenWebMapCard(groups.body, cardIndex)
      : parseLegacyWidgetMapCard(groups.body, cardIndex);
    if (!card) continue;
    if (textBefore.trim()) parts.push({ type: "text", content: textBefore });
    parts.push(card);
    cursor = matchStart + match[0].length;
    cardIndex += 1;
  }
  const trailing = markdown.slice(cursor);
  if (trailing.trim()) parts.push({ type: "text", content: trailing });
  return parts.length ? parts : [{ type: "text", content: markdown }];
}

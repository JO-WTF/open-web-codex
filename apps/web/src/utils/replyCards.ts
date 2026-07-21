export type ReplyCardTextPart = {
  type: "text";
  content: string;
};

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
};

const CARD_FENCE_RE = /(^|\n)(?<fence>`{3,}|~{3,})[ \t]*(?<tag>open-web-card\s+map\.v1|widget)[ \t]*\r?\n(?<body>[\s\S]*?)\r?\n?[ \t]*\k<fence>[ \t]*(?=\n|$)/g;
const MAX_CARD_MARKER_BYTES = 16 * 1024;

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function stableCardId(payload: RawCardPayload, index: number): string {
  const explicit = asString(payload.artifact_id) ?? asString(payload.artifactId) ?? asString(payload.input_ref) ?? asString(payload.inputRef);
  return explicit ? `map-${explicit.replace(/[^a-zA-Z0-9_-]/g, "-").slice(0, 64)}` : `map-card-${index}`;
}

function normalizeStatus(value: unknown): MapReplyCard["status"] {
  if (value === "loading" || value === "ready" || value === "error") return value;
  return "loading";
}

function parseOpenWebMapCard(body: string, index: number): MapReplyCard | null {
  if (new TextEncoder().encode(body).length > MAX_CARD_MARKER_BYTES) return null;
  let payload: RawCardPayload;
  try {
    const parsed = JSON.parse(body) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
    payload = parsed as RawCardPayload;
  } catch {
    return null;
  }
  const title = asString(payload.title) ?? "地图卡片";
  const inputRef = asString(payload.input_ref) ?? asString(payload.inputRef);
  const artifactId = asString(payload.artifact_id) ?? asString(payload.artifactId);
  return {
    type: "card",
    kind: "map.v1",
    id: stableCardId(payload, index),
    title,
    intent: asString(payload.intent),
    inputRef,
    artifactId,
    fallbackText: asString(payload.fallback_text) ?? asString(payload.fallbackText),
    summary: asString(payload.summary),
    status: normalizeStatus(payload.status),
  };
}

function parseLegacyWidgetMapCard(body: string, index: number): MapReplyCard | null {
  if (new TextEncoder().encode(body).length > MAX_CARD_MARKER_BYTES) return null;
  let payload: { widget_type?: unknown; id?: unknown; props?: unknown };
  try {
    const parsed = JSON.parse(body) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
    payload = parsed as typeof payload;
  } catch {
    return null;
  }
  if (payload.widget_type !== "map") return null;
  const props = payload.props && typeof payload.props === "object" && !Array.isArray(payload.props)
    ? payload.props as Record<string, unknown>
    : {};
  const id = asString(payload.id) ?? `legacy-map-card-${index}`;
  return {
    type: "card",
    kind: "map.v1",
    id,
    title: asString(props.title) ?? "地图卡片",
    inputRef: asString(props.input_ref) ?? asString(props.inputRef),
    artifactId: asString(props.artifact_id) ?? asString(props.artifactId),
    fallbackText: asString(props.fallback_text) ?? asString(props.fallbackText),
    summary: props.use_stored_card === true ? "地图数据已存储在服务端，等待平台 Artifact hydration。" : asString(props.summary),
    status: "loading",
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

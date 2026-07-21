import { useState } from "react";
import Expand from "lucide-react/dist/esm/icons/expand";
import MapPinned from "lucide-react/dist/esm/icons/map-pinned";
import type { MapReplyCard as MapReplyCardData } from "../../../utils/replyCards";

type Props = {
  card: MapReplyCardData;
};

function statusLabel(status: MapReplyCardData["status"]) {
  if (status === "ready") return "Ready";
  if (status === "error") return "Failed";
  return "Waiting for GeoJSON Artifact";
}

export default function MapReplyCard({ card }: Props) {
  const [fullscreen, setFullscreen] = useState(false);
  const status = card.status ?? "loading";
  const detail = card.summary ?? card.fallbackText ?? "地图卡片将在平台完成 GeoJSON Artifact hydration 后渲染。";
  const body = (
    <div className={`web-map-card is-${status}`} role="group" aria-label={`Map card: ${card.title}`}>
      <div className="web-map-card-header">
        <div className="web-map-card-title"><MapPinned size={16} aria-hidden="true" /><span>{card.title}</span></div>
        <span className="web-map-card-status">{statusLabel(status)}</span>
        <button type="button" className="web-map-card-fullscreen" onClick={() => setFullscreen(true)} aria-label="Open map card fullscreen">
          <Expand size={14} aria-hidden="true" />
        </button>
      </div>
      <div className="web-map-card-canvas" aria-hidden="true">
        <div className="web-map-card-route" />
        <span className="web-map-card-pin is-a" />
        <span className="web-map-card-pin is-b" />
      </div>
      <div className="web-map-card-body">
        <p>{detail}</p>
        <dl>
          {card.intent && <><dt>Intent</dt><dd>{card.intent}</dd></>}
          {card.inputRef && <><dt>Input ref</dt><dd>{card.inputRef}</dd></>}
          {card.artifactId && <><dt>Artifact</dt><dd>{card.artifactId}</dd></>}
        </dl>
      </div>
    </div>
  );
  return (
    <>
      {body}
      {fullscreen && (
        <div className="web-map-card-modal" role="dialog" aria-modal="true" aria-label={`Fullscreen map card: ${card.title}`}>
          <div className="web-map-card-modal-backdrop" onClick={() => setFullscreen(false)} />
          <div className="web-map-card-modal-panel">
            <button type="button" className="web-map-card-modal-close" onClick={() => setFullscreen(false)}>Close</button>
            {body}
          </div>
        </div>
      )}
    </>
  );
}

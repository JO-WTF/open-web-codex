import { useMemo } from "react";
import {
  indexInlineVisualizationArtifacts,
  segmentInlineVisualizations,
} from "../../../utils/inlineVisualizations";
import type { InlineVisualizationArtifact } from "../../../utils/replyCards";
import ReplyCard from "./ReplyCard";
import SafeMarkdown from "./SafeMarkdown";

type Props = {
  text: string;
  streaming?: boolean;
  onOpenFile?: (path: string) => void;
  variant?: "reply" | "commentary";
  inlineArtifacts?: InlineVisualizationArtifact[];
  showInlineArtifacts?: boolean;
  hiddenInlineArtifactRefs?: string[];
};

export default function AssistantMessage({
  text,
  streaming = false,
  onOpenFile,
  variant = "reply",
  inlineArtifacts,
  showInlineArtifacts = true,
  hiddenInlineArtifactRefs,
}: Props) {
  const commentary = variant === "commentary";
  const segments = useMemo(
    () => segmentInlineVisualizations(text, streaming),
    [streaming, text],
  );
  const artifactIndex = useMemo(
    () => indexInlineVisualizationArtifacts(inlineArtifacts),
    [inlineArtifacts],
  );
  const hiddenArtifactRefs = useMemo(
    () => new Set(hiddenInlineArtifactRefs),
    [hiddenInlineArtifactRefs],
  );
  const hasVisualization = showInlineArtifacts
    && segments.some((segment) => segment.kind !== "markdown");
  return (
    <div
      className={[
        "web-msg-assistant",
        commentary ? "web-msg-commentary" : "",
        hasVisualization ? "has-inline-visualization" : "",
      ].filter(Boolean).join(" ")}
    >
      <div className={`web-msg-assistant-body${commentary ? " web-msg-commentary-body" : ""}`}>
        {segments.map((segment, index) => {
          if (segment.kind === "markdown") {
            return (
              <SafeMarkdown
                key={`markdown-${index}`}
                text={segment.text}
                onOpenFile={onOpenFile}
              />
            );
          }
          if (segment.kind === "artifact") {
            if (!showInlineArtifacts || hiddenArtifactRefs.has(segment.ref)) return null;
            const artifact = artifactIndex.get(segment.ref);
            if (artifact) {
              return (
                <ReplyCard
                  key={`artifact-${segment.ref}-${index}`}
                  card={artifact.card}
                />
              );
            }
            if (streaming) return null;
            return (
              <div
                className="web-inline-visualization-unavailable"
                role="status"
                key={`artifact-unavailable-${segment.ref}-${index}`}
              >
                Visualization unavailable
              </div>
            );
          }
          if (!showInlineArtifacts) return null;
          return (
            <div
              className="web-inline-visualization-unavailable"
              role="status"
              key={`visualization-unavailable-${index}`}
            >
              {segment.kind === "file"
                ? `HTML visualization “${segment.file}” is unavailable in Web.`
                : segment.label}
            </div>
          );
        })}
      </div>
    </div>
  );
}

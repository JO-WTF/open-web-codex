import { memo, useEffect, useState } from "react";
import FileText from "lucide-react/dist/esm/icons/file-text";
import { platformClient } from "../../../../browser/session";
import {
  parseReportArtifactContent,
  type ReportReplyCard as ReportReplyCardData,
} from "../../../utils/replyCards";
import SafeMarkdown from "./SafeMarkdown";

type Props = {
  card: ReportReplyCardData;
};

type ReportLoadState =
  | { status: "loading" }
  | { status: "ready"; markdown: string }
  | { status: "error"; message: string };

async function sha256Hex(value: string): Promise<string> {
  if (!globalThis.crypto?.subtle) {
    throw new Error("Web Crypto is unavailable.");
  }
  const digest = await globalThis.crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(value),
  );
  return Array.from(new Uint8Array(digest), (byte) => (
    byte.toString(16).padStart(2, "0")
  )).join("");
}

function statusLabel(status: ReportLoadState["status"]) {
  if (status === "ready") return "Ready";
  if (status === "error") return "Failed";
  return "Loading";
}

const ReportReplyCard = memo(function ReportReplyCard({ card }: Props) {
  const [loadState, setLoadState] = useState<ReportLoadState>({
    status: "loading",
  });

  useEffect(() => {
    let disposed = false;
    setLoadState({ status: "loading" });
    void platformClient.readReplyArtifact(card.source.url).then(
      (value) => {
        if (disposed) return;
        const report = parseReportArtifactContent(value);
        if (!report) {
          setLoadState({
            status: "error",
            message: "Report content did not match the supported contract.",
          });
          return;
        }
        void sha256Hex(report.markdown).then(
          (actualDigest) => {
            if (disposed) return;
            setLoadState(
              actualDigest === report.markdownSha256
                ? { status: "ready", markdown: report.markdown }
                : {
                  status: "error",
                  message: "Report content failed its integrity check.",
                },
            );
          },
          () => {
            if (!disposed) {
              setLoadState({
                status: "error",
                message: "Report content could not be verified.",
              });
            }
          },
        );
      },
      () => {
        if (!disposed) {
          setLoadState({
            status: "error",
            message: "Report content could not be loaded.",
          });
        }
      },
    );
    return () => {
      disposed = true;
    };
  }, [card.source.url]);

  return (
    <article
      className={`web-report-card is-${loadState.status}`}
      aria-label={`Report: ${card.title}`}
      aria-busy={loadState.status === "loading"}
    >
      <header className="web-report-card-header">
        <div className="web-report-card-title">
          <FileText size={16} aria-hidden="true" />
          <span>{card.title}</span>
        </div>
        <span className="web-report-card-status">
          {statusLabel(loadState.status)}
        </span>
      </header>
      <div className="web-report-card-body">
        {loadState.status === "loading" ? (
          <div className="web-report-card-state" role="status">
            Loading report…
          </div>
        ) : null}
        {loadState.status === "error" ? (
          <div className="web-report-card-state is-error" role="alert">
            <strong>Report unavailable</strong>
            <span>{loadState.message}</span>
          </div>
        ) : null}
        {loadState.status === "ready" ? (
          <SafeMarkdown text={loadState.markdown} />
        ) : null}
      </div>
    </article>
  );
});

export default ReportReplyCard;

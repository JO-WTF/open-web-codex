import { useEffect, useState } from "react";
import Expand from "lucide-react/dist/esm/icons/expand";
import { platformClient } from "../../../../browser/session";

type Props = {
  threadId: string;
  file: string;
  media: "html" | "image";
};

type State =
  | { status: "loading" }
  | { status: "ready"; url: string }
  | { status: "error" };

export default function InlineVisualizationFile({ threadId, file, media }: Props) {
  const [state, setState] = useState<State>({ status: "loading" });
  const [fullscreen, setFullscreen] = useState(false);

  useEffect(() => {
    let active = true;
    let objectUrl: string | null = null;
    setState({ status: "loading" });
    void platformClient.readInlineVisualization(threadId, file).then(
      ({ blob }) => {
        if (!active) return;
        objectUrl = URL.createObjectURL(blob);
        setState({ status: "ready", url: objectUrl });
      },
      () => {
        if (active) setState({ status: "error" });
      },
    );
    return () => {
      active = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [file, threadId]);

  if (state.status === "loading") {
    return (
      <div className="web-inline-visualization-file is-loading" role="status">
        Loading visualization…
      </div>
    );
  }
  if (state.status === "error") {
    return (
      <div className="web-inline-visualization-unavailable" role="alert">
        Visualization unavailable
      </div>
    );
  }
  if (media === "image") {
    return (
      <figure className="web-inline-visualization-file is-image">
        <img src={state.url} alt={`${file} visualization`} loading="lazy" decoding="async" />
      </figure>
    );
  }

  const htmlCard = (fullscreenCard = false) => (
    <section
      className="web-inline-visualization-file is-html"
      role="group"
      aria-label={`HTML visualization: ${file}`}
    >
      <div className="web-inline-visualization-header">
        <span className="web-inline-visualization-title">{file}</span>
        {!fullscreenCard ? (
          <button
            type="button"
            className="web-inline-visualization-fullscreen"
            onClick={() => setFullscreen(true)}
            aria-label="Open HTML visualization fullscreen"
          >
            <Expand size={16} aria-hidden="true" />
            <span>全屏</span>
          </button>
        ) : null}
      </div>
      <iframe
        className="web-inline-visualization-frame"
        src={state.url}
        title={`${file} visualization`}
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
      />
    </section>
  );

  return (
    <>
      {htmlCard()}
      {fullscreen ? (
        <div
          className="web-inline-visualization-modal"
          role="dialog"
          aria-modal="true"
          aria-label={`Fullscreen HTML visualization: ${file}`}
        >
          <div
            className="web-inline-visualization-modal-backdrop"
            onClick={() => setFullscreen(false)}
          />
          <div className="web-inline-visualization-modal-panel">
            <button
              type="button"
              className="web-inline-visualization-modal-close"
              onClick={() => setFullscreen(false)}
            >
              Close
            </button>
            {htmlCard(true)}
          </div>
        </div>
      ) : null}
    </>
  );
}

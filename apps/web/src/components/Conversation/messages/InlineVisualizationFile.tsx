import { useEffect, useState } from "react";
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
  return (
    <iframe
      className="web-inline-visualization-file is-html"
      src={state.url}
      title={`${file} visualization`}
      sandbox="allow-scripts"
      referrerPolicy="no-referrer"
    />
  );
}

import { describe, expect, it } from "vitest";
import { parseInlineVisualizationArtifact } from "./replyCards";

describe("parseInlineVisualizationArtifact", () => {
  it("normalizes the Server-projected typed renderer contract", () => {
    const artifact = parseInlineVisualizationArtifact({
      ref: "map-batch",
      renderer: {
        kind: "map.v2",
        payload: {
        title: "Batch geocode",
        intent: "visualization",
        status: "ready",
        fallback_text: "Three locations",
        viewport: { mode: "camera", center: [-122.08, 37.42], zoom: 10 },
        sources: [{
          id: "locations",
          data: {
            type: "inline",
            format: "geojson",
            geojson: { type: "FeatureCollection", features: [] },
          },
        }],
        layers: [{
          id: "points",
          source: "locations",
          geometry: "point",
          label_property: "label",
          hover: {
            title_property: "label",
            fields: [{
              property: "population",
              label: "Population",
            }],
          },
          style: {
            color: "#ef4444",
            opacity: 0.8,
            shape: "diamond",
            size: 22,
            stroke_color: "#ffffff",
            stroke_width: 2,
          },
        }],
        },
      },
    });

    expect(artifact).toMatchObject({
      ref: "map-batch",
      rendererKind: "map.v2",
      card: {
        type: "card",
        kind: "map.v2",
        id: "map-batch",
        title: "Batch geocode",
        fallbackText: "Three locations",
        status: "ready",
        viewport: { mode: "camera", center: [-122.08, 37.42], zoom: 10 },
        layers: [{
          geometry: "point",
          labelProperty: "label",
          hover: {
            titleProperty: "label",
            fields: [{
              property: "population",
              label: "Population",
            }],
          },
          style: {
            color: "#ef4444",
            opacity: 0.8,
            shape: "diamond",
            size: 22,
            strokeColor: "#ffffff",
            strokeWidth: 2,
          },
        }],
      },
    });
  });

  it("normalizes custom icon presentation fields", () => {
    const artifact = parseInlineVisualizationArtifact({
      ref: "map-icons",
      renderer: {
        kind: "map.v2",
        payload: {
          title: "Icons",
          intent: "visualization",
          status: "ready",
          viewport: { mode: "fit" },
          sources: [{
            id: "locations",
            data: {
              type: "inline",
              format: "geojson",
              geojson: { type: "FeatureCollection", features: [] },
            },
          }],
          layers: [{
            id: "icons",
            source: "locations",
            geometry: "point",
            style: {
              opacity: 0.9,
              icon: {
                url: "https://cdn.example.com/marker.png",
                scale: 0.75,
                anchor: "bottom",
                rotation: 15,
                allow_overlap: true,
              },
            },
          }],
        },
      },
    });

    expect(artifact?.card.layers[0]).toMatchObject({
      geometry: "point",
      style: {
        opacity: 0.9,
        icon: {
          url: "https://cdn.example.com/marker.png",
          scale: 0.75,
          anchor: "bottom",
          rotation: 15,
          allowOverlap: true,
        },
      },
    });
  });

  it("does not interpret Tool text or an unsupported renderer", () => {
    expect(parseInlineVisualizationArtifact({
      content: [{
        type: "text",
        text: "{\"renderer\":{\"kind\":\"map.v2\"}}",
      }],
    })).toBeNull();
    expect(parseInlineVisualizationArtifact({
      ref: "chart-one",
      renderer: { kind: "chart.v1", payload: {} },
    })).toBeNull();
  });
});

import { describe, expect, it } from "vitest";
import type { MapReplyCard as MapReplyCardData } from "../../../utils/replyCards";
import {
  dataBoundsForSources,
  mapStyleForToken,
  sameMapReplyCard,
} from "./MapReplyCard";

function card(overrides: Partial<MapReplyCardData>): MapReplyCardData {
  return {
    type: "card",
    kind: "map.v2",
    id: "map-1",
    title: "Map",
    intent: "visualization",
    status: "ready",
    viewport: { mode: "fit" },
    sources: [{
      id: "data",
      data: {
        type: "inline",
        format: "geojson",
        geojson: { type: "FeatureCollection", features: [] },
      },
    }],
    layers: [{
      id: "points",
      source: "data",
      geometry: "point",
      style: {},
    }],
    ...overrides,
  };
}

describe("dataBoundsForSources", () => {
  it("expands a single point into a usable viewport", () => {
    expect(dataBoundsForSources([{
      id: "places",
      data: {
        type: "FeatureCollection",
        features: [{
          type: "Feature",
          properties: {},
          geometry: { type: "Point", coordinates: [106.8456, -6.2088] },
        }],
      },
    }])).toEqual([106.7656, -6.2888, 106.9256, -6.1288]);
  });

  it("fits all loaded GeoJSON sources", () => {
    expect(dataBoundsForSources([
      {
        id: "line",
        data: {
          type: "LineString",
          coordinates: [[110, -8], [120, 2]],
        },
      },
      {
        id: "polygon",
        data: {
          type: "Polygon",
          coordinates: [[[100, -10], [105, -10], [105, -5], [100, -10]]],
        },
      },
    ])).toEqual([100, -10, 120, 2]);
  });
});

describe("Mapbox map.v2 rendering data", () => {
  it("keeps an unchanged parsed card stable across parent message updates", () => {
    const first = card({
      title: "上海市行政区划边界",
      layers: [{
        id: "area",
        source: "data",
        geometry: "polygon",
        style: { fillColor: "#0050c8", fillOpacity: 0.3 },
      }],
    });
    const reparsed = card({
      title: "上海市行政区划边界",
      layers: [{
        id: "area",
        source: "data",
        geometry: "polygon",
        style: { fillColor: "#0050c8", fillOpacity: 0.3 },
      }],
    });

    expect(reparsed).not.toBe(first);
    expect(sameMapReplyCard(first, reparsed)).toBe(true);
    expect(sameMapReplyCard(first, { ...reparsed, title: "更新后的地图" })).toBe(false);
  });

  it("uses Mapbox Streets only with a browser token", () => {
    expect(mapStyleForToken("pk.public-token")).toBe(
      "mapbox://styles/mapbox/streets-v12",
    );
    expect(mapStyleForToken("")).toBeNull();
  });

  it("preserves an explicit camera zoom in the parsed card", () => {
    expect(card({
      viewport: {
        mode: "camera",
        center: [121.4737, 31.2304],
        zoom: 12,
        bearing: 15,
        pitch: 30,
      },
    }).viewport).toEqual({
      mode: "camera",
      center: [121.4737, 31.2304],
      zoom: 12,
      bearing: 15,
      pitch: 30,
    });
  });
});

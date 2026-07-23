import { describe, expect, it } from "vitest";
import type { MapReplyCard as MapReplyCardData } from "../../../utils/replyCards";
import { dataBoundsForCard } from "./MapReplyCard";

function card(overrides: Partial<MapReplyCardData>): MapReplyCardData {
  return { type: "card", kind: "map.v1", id: "map-1", title: "Map", ...overrides };
}

describe("dataBoundsForCard", () => {
  it("expands a single point into a usable viewport", () => {
    expect(dataBoundsForCard(card({
      points: [{ latitude: -6.2088, longitude: 106.8456, label: "Jakarta" }],
    }))).toEqual([106.7656, -6.2888, 106.9256, -6.1288]);
  });

  it("fits all inline geometries", () => {
    expect(dataBoundsForCard(card({
      points: [{ latitude: -6, longitude: 106 }],
      lines: [{ coordinates: [[110, -8], [120, 2]] }],
      polygons: [{ coordinates: [[[100, -10], [105, -10], [105, -5], [100, -10]]] }],
    }))).toEqual([100, -10, 120, 2]);
  });

  it("reads coordinates from GeoJSON feature collections", () => {
    expect(dataBoundsForCard(card({
      geojson: {
        type: "FeatureCollection",
        features: [{ type: "Feature", properties: {}, geometry: { type: "Point", coordinates: [115, -3] } }],
      },
    }))).toEqual([114.92, -3.08, 115.08, -2.92]);
  });
});

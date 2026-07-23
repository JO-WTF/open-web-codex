import { describe, expect, it } from "vitest";
import type { MapReplyCard as MapReplyCardData } from "../../../utils/replyCards";
import {
  dataBoundsForCard,
  featureCollectionForCard,
  initialMapViewport,
  mapStyleForToken,
  sameMapReplyCard,
} from "./MapReplyCard";

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

describe("Mapbox card rendering data", () => {
  it("keeps an unchanged parsed card stable across parent message updates", () => {
    const first = card({
      title: "上海市行政区划边界",
      polygons: [{
        id: "shanghai",
        color: "#0050c8",
        coordinates: [[[121, 31], [122, 31], [122, 32], [121, 31]]],
      }],
    });
    const reparsed = card({
      title: "上海市行政区划边界",
      polygons: [{
        id: "shanghai",
        color: "#0050c8",
        coordinates: [[[121, 31], [122, 31], [122, 32], [121, 31]]],
      }],
    });

    expect(reparsed).not.toBe(first);
    expect(sameMapReplyCard(first, reparsed)).toBe(true);
    expect(sameMapReplyCard(first, {
      ...reparsed,
      title: "更新后的地图",
    })).toBe(false);
  });

  it("converts points, lines, and polygons into one GeoJSON collection", () => {
    const collection = featureCollectionForCard(card({
      points: [{ latitude: 31.2304, longitude: 121.4737, label: "上海" }],
      lines: [{ coordinates: [[116.4074, 39.9042], [121.4737, 31.2304]] }],
      polygons: [{
        coordinates: [[[116, 39], [117, 39], [117, 40], [116, 39]]],
      }],
    }));

    expect(collection?.features.map((feature) => (
      (feature.geometry as { type: string }).type
    ))).toEqual(["Point", "LineString", "Polygon"]);
  });

  it("uses Mapbox Streets with a public token", () => {
    expect(mapStyleForToken("pk.public-token")).toBe(
      "mapbox://styles/mapbox/streets-v12",
    );
  });

  it("does not initialize Mapbox without a public browser token", () => {
    expect(mapStyleForToken("")).toBeNull();
  });

  it("uses the final fitted viewport during Mapbox construction", () => {
    expect(initialMapViewport([121.49, 31.23, 121.51, 31.25], false, 12)).toEqual({
      bounds: [121.49, 31.23, 121.51, 31.25],
      fitBoundsOptions: {
        padding: 40,
        maxZoom: 12,
        duration: 0,
      },
    });
    expect(initialMapViewport([121.49, 31.23, 121.51, 31.25], true)).toMatchObject({
      fitBoundsOptions: {
        padding: 72,
        maxZoom: 14,
      },
    });
  });
});

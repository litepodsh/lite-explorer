import { invoke } from "@tauri-apps/api/core";

export type GeoFeature = {
  name: string;
  kind: "Point" | "LineString" | "Polygon" | string;
  points: [number, number][];
};

export type GeoPreview = {
  format: string;
  features: GeoFeature[];
  bounds: [number, number, number, number] | null;
  pointCount: number;
};

/** Parses a `.geojson`/`.kml`/`.gpx` file into plot-ready features. */
export function openGeo(path: string): Promise<GeoPreview> {
  return invoke<GeoPreview>("open_geo", { path });
}

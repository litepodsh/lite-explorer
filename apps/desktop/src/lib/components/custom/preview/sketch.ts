import { invoke } from "@tauri-apps/api/core";

export type SketchLayer = {
  name: string;
  kind: string;
  x: number;
  y: number;
  width: number;
  height: number;
  visible: boolean;
  text: string;
};

export type SketchPage = {
  name: string;
  width: number;
  height: number;
  layers: SketchLayer[];
};

export type SketchPreview = {
  pages: SketchPage[];
};

/** Reads pages and top-level layers of a `.sketch` document. */
export function openSketch(path: string): Promise<SketchPreview> {
  return invoke<SketchPreview>("open_sketch", { path });
}

import { invoke } from "@tauri-apps/api/core";

export type MobiPreview = {
  title: string;
  paragraphs: string[];
  truncated: boolean;
};

/** Extracts text from a Kindle `.mobi`/`.azw3` book. */
export function openMobi(path: string): Promise<MobiPreview> {
  return invoke<MobiPreview>("open_mobi", { path });
}

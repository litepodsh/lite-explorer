import { invoke } from "@tauri-apps/api/core";

export type RtfDocument = {
  html: string;
};

/** Converts a local RTF file into sanitized HTML. */
export function openRtf(path: string): Promise<RtfDocument> {
  return invoke<RtfDocument>("open_rtf", { path });
}

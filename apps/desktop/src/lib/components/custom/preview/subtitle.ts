import { invoke } from "@tauri-apps/api/core";

export type SubtitleCue = {
  index: number;
  start: string;
  end: string;
  startMs: number;
  endMs: number;
  text: string;
};

export type SubtitlePreview = {
  format: string;
  cues: SubtitleCue[];
};

/** Parses a `.srt` or `.vtt` subtitle file into cues. */
export function openSubtitle(path: string): Promise<SubtitlePreview> {
  return invoke<SubtitlePreview>("open_subtitle", { path });
}

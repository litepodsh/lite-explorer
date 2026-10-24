import {
  extensionOf,
  mediaKindOf,
  missingEncoders,
  type AnimationFormat,
  type AudioFormat,
  type Container,
  type FfmpegStatus,
  type MediaInfo,
  type Speed,
  type TranscodeOptions,
  type Trim,
} from "./transcode.js";

export type Tab = "video" | "audio" | "animation";
export type PresetId = "small" | "balanced" | "high" | "social";

export type DialogState = {
  tab: Tab;
  container: Container;
  /** The preset the video settings match, or `null` once they were changed by hand. */
  preset: PresetId | null;
  quality: number;
  speed: Speed;
  maxHeight: number | null;
  fps: number | null;
  mute: boolean;
  hardware: boolean;
  audioFormat: AudioFormat;
  bitrate: number;
  animationFormat: AnimationFormat;
  width: number;
  animationFps: number;
  repeat: boolean;
  animationQuality: number;
  /** Text as typed ("1:30"). Empty means no trim on that side. */
  start: string;
  end: string;
  replaceOriginal: boolean;
};

type Preset = {
  id: PresetId;
  label: string;
  hint: string;
  quality: number;
  speed: Speed;
  maxHeight: number | null;
  fps: number | null;
};

export const PRESETS: readonly Preset[] = [
  { id: "small", label: "Small file", hint: "720p, lower quality", quality: 35, speed: "balanced", maxHeight: 720, fps: 30 },
  { id: "balanced", label: "Balanced", hint: "Original size, good quality", quality: 55, speed: "balanced", maxHeight: null, fps: null },
  { id: "high", label: "High quality", hint: "Original size, near lossless", quality: 80, speed: "slow", maxHeight: null, fps: null },
  { id: "social", label: "Social 1080p", hint: "1080p at 30 fps", quality: 60, speed: "balanced", maxHeight: 1080, fps: 30 },
];

export const HEIGHTS = [
  { value: null, label: "Original" },
  { value: 1080, label: "1080p" },
  { value: 720, label: "720p" },
  { value: 480, label: "480p" },
] as const;

export const FRAME_RATES = [
  { value: null, label: "Original" },
  { value: 60, label: "60" },
  { value: 30, label: "30" },
  { value: 24, label: "24" },
] as const;

export const BITRATES = [96, 128, 192, 256, 320] as const;
export const GIF_WIDTHS = [320, 480, 640, 800] as const;
export const GIF_FRAME_RATES = [8, 12, 15, 24] as const;

const LOSSLESS: readonly AudioFormat[] = ["flac", "wav"];

export function isLossless(format: AudioFormat): boolean {
  return LOSSLESS.includes(format);
}

export function applyPreset(state: DialogState, id: PresetId): DialogState {
  const preset = PRESETS.find((candidate) => candidate.id === id);
  if (!preset) return state;
  return {
    ...state,
    preset: id,
    quality: preset.quality,
    speed: preset.speed,
    maxHeight: preset.maxHeight,
    fps: preset.fps,
  };
}

/** Starts on the audio tab for audio files, and on the preset most people want. */
export function defaultState(names: readonly string[]): DialogState {
  const base: DialogState = {
    tab: mediaKindOf(names) === "audio" ? "audio" : "video",
    container: "mp4",
    preset: "balanced",
    quality: 55,
    speed: "balanced",
    maxHeight: null,
    fps: null,
    mute: false,
    hardware: false,
    audioFormat: "mp3",
    bitrate: 192,
    animationFormat: "gif",
    width: 480,
    animationFps: 12,
    repeat: true,
    animationQuality: 75,
    start: "",
    end: "",
    replaceOriginal: false,
  };
  // An audio file already in MP3 is more likely to go to another format.
  if (base.tab === "audio" && names.length > 0 && extensionOf(names[0]) === "mp3") {
    base.audioFormat = "m4a";
  }
  return applyPreset(base, "balanced");
}

export type TimeResult = { ok: true; value: number | null } | { ok: false };

/** "90", "1:30", "1:30.5" and "1:02:03" give seconds; an empty text means no limit. */
export function parseTime(text: string): TimeResult {
  const trimmed = text.trim();
  if (trimmed === "") return { ok: true, value: null };
  const parts = trimmed.split(":");
  if (parts.length > 3 || parts.some((part) => !/^\d+(\.\d+)?$/.test(part))) return { ok: false };
  const numbers = parts.map(Number);
  // Only the last part may have decimals, and minutes and seconds stay under 60 once there is a larger unit.
  if (numbers.slice(0, -1).some((number) => !Number.isInteger(number))) return { ok: false };
  if (numbers.length > 1 && numbers.slice(1).some((number) => number >= 60)) return { ok: false };
  const seconds = numbers.reduce((total, number) => total * 60 + number, 0);
  return Number.isFinite(seconds) ? { ok: true, value: seconds } : { ok: false };
}

/** 90 gives "1:30", 5.5 gives "0:05.5" and 3725 gives "1:02:05". */
export function formatTime(seconds: number): string {
  const whole = Math.floor(seconds);
  const fraction = seconds - whole;
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const rest = whole % 60;
  const decimals = fraction >= 0.05 ? `.${Math.round(fraction * 10) % 10}` : "";
  const two = (value: number) => String(value).padStart(2, "0");
  return hours > 0
    ? `${hours}:${two(minutes)}:${two(rest)}${decimals}`
    : `${minutes}:${two(rest)}${decimals}`;
}

export type TrimResult = { trim: Trim; error: string | null };

export function trimOf(state: DialogState, duration: number | null): TrimResult {
  const none: Trim = { start: null, end: null };
  const start = parseTime(state.start);
  const end = parseTime(state.end);
  if (!start.ok || !end.ok) {
    return { trim: none, error: "Write times like 1:30 or 90" };
  }
  const trim: Trim = { start: start.value, end: end.value };
  if (trim.start !== null && trim.end !== null && trim.end <= trim.start) {
    return { trim: none, error: "The end must come after the start" };
  }
  if (duration !== null && trim.start !== null && trim.start >= duration) {
    return { trim: none, error: "The start is past the end of the clip" };
  }
  return { trim, error: null };
}

/** How long the result lasts after trimming. `null` when the file doesn't say. */
export function clipLength(duration: number | null, trim: Trim): number | null {
  if (duration === null) return null;
  const start = Math.min(trim.start ?? 0, duration);
  const end = Math.min(trim.end ?? duration, duration);
  return Math.max(0, end - start);
}

export function toOptions(state: DialogState, trim: Trim): TranscodeOptions {
  switch (state.tab) {
    case "video":
      return {
        mode: "video",
        container: state.container,
        quality: state.quality,
        speed: state.speed,
        maxHeight: state.maxHeight,
        fps: state.fps,
        mute: state.mute,
        hardware: state.hardware,
        trim,
      };
    case "audio":
      return { mode: "audio", format: state.audioFormat, bitrate: state.bitrate, trim };
    case "animation":
      return {
        mode: "animation",
        format: state.animationFormat,
        width: state.width,
        fps: state.animationFps,
        repeat: state.repeat,
        quality: state.animationQuality,
        trim,
      };
  }
}

/** Exact size of a lossy audio file: bitrate times duration. `null` for lossless formats. */
export function audioSize(seconds: number | null, format: AudioFormat, bitrate: number): number | null {
  if (seconds === null || isLossless(format)) return null;
  return Math.round((seconds * bitrate * 1000) / 8);
}

/** Name suffix for results that stay in the same container, so they don't read as copies. */
export function suffixFor(state: DialogState): string | null {
  return state.tab === "video" && state.preset === "small" ? " (compressed)" : null;
}

export function warningsFor(
  state: DialogState,
  info: MediaInfo | null,
  status: FfmpegStatus | null,
  options: TranscodeOptions,
  lengthSeconds: number | null,
): string[] {
  const warnings: string[] = [];
  if (info && state.tab !== "audio" && !info.video) {
    warnings.push("This file has no video.");
  }
  if (info && state.tab === "audio" && !info.audio) {
    warnings.push("This file has no audio.");
  }
  for (const name of missingEncoders(status, options)) {
    warnings.push(`This ffmpeg can’t create that format (${name} is missing).`);
  }
  if (state.tab === "animation" && state.animationFormat === "gif" && lengthSeconds !== null && lengthSeconds > 15) {
    warnings.push("A GIF this long will be very large. Trim the clip or use WebP.");
  }
  if (state.tab === "video" && state.container === "webm" && state.speed === "slow") {
    warnings.push("VP9 at the slowest speed can take a long time.");
  }
  return warnings;
}

/** The second to show in the preview: the middle of what will be converted. */
export function defaultFrameTime(duration: number | null, trim: Trim): number {
  const length = clipLength(duration, trim);
  if (length === null || duration === null) return 0;
  const start = Math.min(trim.start ?? 0, duration);
  return Math.floor(start + length / 2);
}

/** Changes whenever something that affects the result changes, so a stale estimate can be dropped. */
export function settingsKey(path: string, options: TranscodeOptions): string {
  return `${path}\n${JSON.stringify(options)}`;
}

export function isValid(state: DialogState, duration: number | null): boolean {
  if (trimOf(state, duration).error) return false;
  switch (state.tab) {
    case "video":
      return state.quality >= 0 && state.quality <= 100;
    case "audio":
      return state.bitrate >= 32 && state.bitrate <= 512;
    case "animation":
      return state.width >= 16 && state.width <= 4096 && state.animationFps >= 1 && state.animationFps <= 60;
  }
}

export type QuickMediaKind = "mp4" | "compress" | "gif" | "mp3" | "m4a" | "flac" | "wav";

/** Quick Convert entries for a selection of videos or of audio files. */
export function quickMediaKindsFor(names: readonly string[]): QuickMediaKind[] {
  const kind = mediaKindOf(names);
  const allAre = (extension: string) => names.every((name) => extensionOf(name) === extension);
  if (kind === "video") {
    const kinds: QuickMediaKind[] = ["mp4", "compress", "gif", "mp3"];
    return kinds.filter((entry) => !(entry === "mp4" && allAre("mp4")));
  }
  if (kind === "audio") {
    const kinds: QuickMediaKind[] = ["mp3", "m4a", "flac", "wav"];
    return kinds.filter((entry) => !allAre(entry));
  }
  return [];
}

export function quickMediaLabel(kind: QuickMediaKind, source: "video" | "audio"): string {
  switch (kind) {
    case "mp4":
      return "MP4 (H.264)";
    case "compress":
      return "Compress";
    case "gif":
      return "GIF";
    case "mp3":
      return source === "video" ? "Extract MP3" : "MP3";
    case "m4a":
      return "M4A (AAC)";
    case "flac":
      return "FLAC";
    case "wav":
      return "WAV";
  }
}

export function quickMediaOptions(kind: QuickMediaKind): { options: TranscodeOptions; suffix: string | null } {
  const none: Trim = { start: null, end: null };
  switch (kind) {
    case "mp4":
      return {
        options: { mode: "video", container: "mp4", quality: 60, speed: "balanced", maxHeight: null, fps: null, mute: false, hardware: false, trim: none },
        suffix: null,
      };
    case "compress":
      return {
        options: { mode: "video", container: "mp4", quality: 35, speed: "balanced", maxHeight: 720, fps: 30, mute: false, hardware: false, trim: none },
        suffix: " (compressed)",
      };
    case "gif":
      return {
        options: { mode: "animation", format: "gif", width: 480, fps: 12, repeat: true, quality: 75, trim: none },
        suffix: null,
      };
    case "mp3":
      return { options: { mode: "audio", format: "mp3", bitrate: 192, trim: none }, suffix: null };
    case "m4a":
      return { options: { mode: "audio", format: "m4a", bitrate: 192, trim: none }, suffix: null };
    case "flac":
      return { options: { mode: "audio", format: "flac", bitrate: 192, trim: none }, suffix: null };
    case "wav":
      return { options: { mode: "audio", format: "wav", bitrate: 192, trim: none }, suffix: null };
  }
}

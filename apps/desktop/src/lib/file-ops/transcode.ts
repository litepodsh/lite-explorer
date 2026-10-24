import { invoke } from "@tauri-apps/api/core";
import type { ConvertOutcome } from "./convert.js";

export type Container = "mp4" | "webm" | "mov" | "mkv";
export type Speed = "fast" | "balanced" | "slow";
export type AudioFormat = "mp3" | "m4a" | "flac" | "wav" | "opus";
export type AnimationFormat = "gif" | "webp";

/** Seconds. `null` means the start or the end of the file. */
export type Trim = { start: number | null; end: number | null };

/** Mirror of `TranscodeOptions` in `src-tauri/src/explorer/transcode/options.rs`. */
export type TranscodeOptions =
  | {
      mode: "video";
      container: Container;
      quality: number;
      speed: Speed;
      maxHeight: number | null;
      fps: number | null;
      mute: boolean;
      /** Use the graphics hardware for H.264 when the machine has it. */
      hardware: boolean;
      trim: Trim;
    }
  | { mode: "audio"; format: AudioFormat; bitrate: number; trim: Trim }
  | {
      mode: "animation";
      format: AnimationFormat;
      width: number;
      fps: number;
      repeat: boolean;
      quality: number;
      trim: Trim;
    };

export type MediaInfo = {
  duration: number | null;
  size: number;
  format: string;
  bitRate: number | null;
  video: {
    codec: string;
    width: number;
    height: number;
    fps: number | null;
    bitRate: number | null;
  } | null;
  audio: {
    codec: string;
    channels: number;
    sampleRate: number | null;
    bitRate: number | null;
  } | null;
};

export type FfmpegStatus = {
  available: boolean;
  source: "managed" | "system" | null;
  version: string | null;
  encoders: string[];
  canDownload: boolean;
  downloadSize: number | null;
  downloadVersion: string;
  /** The graphics encoder that works on this machine. */
  hardware: "videotoolbox" | "nvenc" | null;
};

const VIDEO_EXTENSIONS = [
  "mp4", "m4v", "mov", "webm", "ogv", "mkv", "avi", "wmv", "flv", "mpg", "mpeg", "m2ts", "mts", "3gp",
];
const AUDIO_EXTENSIONS = [
  "mp3", "m4a", "aac", "wav", "flac", "ogg", "oga", "opus", "wma", "aiff", "aif", "ac3",
];

export function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot < 0 ? "" : name.slice(dot + 1).toLowerCase();
}

/** "video" or "audio" when every name is one of that kind; `null` for a mix or anything else. */
export function mediaKindOf(names: readonly string[]): "video" | "audio" | null {
  if (names.length === 0) return null;
  if (names.every((name) => VIDEO_EXTENSIONS.includes(extensionOf(name)))) return "video";
  if (names.every((name) => AUDIO_EXTENSIONS.includes(extensionOf(name)))) return "audio";
  return null;
}

/** The encoders a job needs from ffmpeg. Mirrors `required_encoders` in Rust. */
export function requiredEncoders(options: TranscodeOptions): string[] {
  switch (options.mode) {
    case "video": {
      const [video, audio] =
        options.container === "webm" ? ["libvpx-vp9", "libopus"] : ["libx264", "aac"];
      return options.mute ? [video] : [video, audio];
    }
    case "audio":
      return [
        { mp3: "libmp3lame", m4a: "aac", flac: "flac", wav: "pcm_s16le", opus: "libopus" }[
          options.format
        ],
      ];
    case "animation":
      return [options.format === "gif" ? "gif" : "libwebp_anim"];
  }
}

/** Encoders the job needs that this ffmpeg build does not have. */
export function missingEncoders(status: FfmpegStatus | null, options: TranscodeOptions): string[] {
  if (!status?.available) return [];
  return requiredEncoders(options).filter((name) => !status.encoders.includes(name));
}

export function ffmpegStatus(): Promise<FfmpegStatus> {
  return invoke<FfmpegStatus>("ffmpeg_status");
}

/** Progress arrives as `transfer-progress` events with this job id. */
export function ffmpegDownload(jobId: string): Promise<FfmpegStatus> {
  return invoke<FfmpegStatus>("ffmpeg_download", { jobId });
}

export function mediaProbe(path: string): Promise<MediaInfo> {
  return invoke<MediaInfo>("media_probe", { path });
}

/**
 * About how many bytes the conversion would produce, from a few short samples. Takes seconds;
 * `cancel_transfer` with the same job id stops it (the promise then rejects with "Cancelled").
 */
export function transcodeEstimate(path: string, options: TranscodeOptions, jobId: string): Promise<number> {
  return invoke<number>("transcode_estimate", { jobId, path, options });
}

/** A picture of the video at `time` seconds, as it is or as the conversion would make it. */
export async function transcodeFrame(
  path: string,
  options: TranscodeOptions,
  time: number,
  converted: boolean,
): Promise<Blob> {
  const buffer = await invoke<ArrayBuffer>("transcode_frame", { path, options, time, converted });
  return new Blob([buffer], { type: "image/png" });
}

/** Progress and cancel go through the shared `transfer-progress` events (kind `convert`). */
export function transcodeRun(
  paths: string[],
  options: TranscodeOptions,
  suffix: string | null,
  replaceOriginal: boolean,
): Promise<ConvertOutcome> {
  return invoke<ConvertOutcome>("transcode_run", {
    jobId: crypto.randomUUID(),
    paths,
    options,
    suffix,
    replaceOriginal,
  });
}

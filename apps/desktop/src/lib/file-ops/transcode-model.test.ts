import { describe, expect, test } from "bun:test";
import {
  BITRATES,
  PRESETS,
  applyPreset,
  audioSize,
  clipLength,
  defaultFrameTime,
  defaultState,
  formatTime,
  isLossless,
  isValid,
  parseTime,
  quickMediaKindsFor,
  quickMediaLabel,
  quickMediaOptions,
  settingsKey,
  suffixFor,
  toOptions,
  trimOf,
  warningsFor,
} from "./transcode-model.js";
import {
  extensionOf,
  mediaKindOf,
  missingEncoders,
  requiredEncoders,
  type FfmpegStatus,
  type MediaInfo,
  type TranscodeOptions,
} from "./transcode.js";

const status = (encoders: string[], available = true): FfmpegStatus => ({
  available,
  source: "system",
  version: "9.0.2",
  encoders,
  canDownload: true,
  downloadSize: 1,
  downloadVersion: "9.0.2",
  hardware: null,
});

const info = (overrides: Partial<MediaInfo> = {}): MediaInfo => ({
  duration: 60,
  size: 1000,
  format: "mov,mp4",
  bitRate: null,
  video: { codec: "h264", width: 1920, height: 1080, fps: 30, bitRate: null },
  audio: { codec: "aac", channels: 2, sampleRate: 48000, bitRate: null },
  ...overrides,
});

describe("mediaKindOf", () => {
  test("recognizes videos and audio files by extension", () => {
    expect(mediaKindOf(["a.MP4", "b.mkv"])).toBe("video");
    expect(mediaKindOf(["a.mp3", "b.FLAC"])).toBe("audio");
    expect(mediaKindOf(["a.m4v"])).toBe("video");
  });

  test("a mix, other files and an empty selection are neither", () => {
    expect(mediaKindOf(["a.mp4", "b.mp3"])).toBeNull();
    expect(mediaKindOf(["a.png"])).toBeNull();
    expect(mediaKindOf(["notes.txt", "a.mp4"])).toBeNull();
    expect(mediaKindOf([])).toBeNull();
    expect(mediaKindOf(["main.ts"])).toBeNull();
  });

  test("extensionOf", () => {
    expect(extensionOf("Clip.MOV")).toBe("mov");
    expect(extensionOf("noext")).toBe("");
  });
});

describe("encoders", () => {
  const video = (container: "mp4" | "webm", mute: boolean): TranscodeOptions => ({
    mode: "video",
    container,
    quality: 50,
    speed: "balanced",
    maxHeight: null,
    fps: null,
    mute,
    hardware: false,
    trim: { start: null, end: null },
  });

  test("each job lists what it needs", () => {
    expect(requiredEncoders(video("mp4", false))).toEqual(["libx264", "aac"]);
    expect(requiredEncoders(video("webm", false))).toEqual(["libvpx-vp9", "libopus"]);
    expect(requiredEncoders(video("mp4", true))).toEqual(["libx264"]);
  });

  test("only what the build lacks is reported", () => {
    expect(missingEncoders(status(["libx264"]), video("mp4", false))).toEqual(["aac"]);
    expect(missingEncoders(status(["libx264", "aac"]), video("mp4", false))).toEqual([]);
    expect(missingEncoders(null, video("mp4", false))).toEqual([]);
    expect(missingEncoders(status([], false), video("mp4", false))).toEqual([]);
  });
});

describe("parseTime", () => {
  test("accepts seconds, minutes and hours", () => {
    expect(parseTime("90")).toEqual({ ok: true, value: 90 });
    expect(parseTime("1:30")).toEqual({ ok: true, value: 90 });
    expect(parseTime("1:30.5")).toEqual({ ok: true, value: 90.5 });
    expect(parseTime("1:02:03")).toEqual({ ok: true, value: 3723 });
    expect(parseTime("0:00")).toEqual({ ok: true, value: 0 });
    expect(parseTime("  2:00  ")).toEqual({ ok: true, value: 120 });
  });

  test("empty means no limit", () => {
    expect(parseTime("")).toEqual({ ok: true, value: null });
    expect(parseTime("   ")).toEqual({ ok: true, value: null });
  });

  test("rejects anything else", () => {
    for (const text of ["abc", "1:2:3:4", "-5", "1:60", "1.5:30", "1:", ":30", "1,5", "1:30:60"]) {
      expect(parseTime(text)).toEqual({ ok: false });
    }
  });
});

describe("formatTime", () => {
  test("minutes, hours and tenths", () => {
    expect(formatTime(90)).toBe("1:30");
    expect(formatTime(5)).toBe("0:05");
    expect(formatTime(5.5)).toBe("0:05.5");
    expect(formatTime(3725)).toBe("1:02:05");
    expect(formatTime(0)).toBe("0:00");
  });

  test("round-trips through parseTime", () => {
    for (const seconds of [0, 7, 59.5, 90, 3723]) {
      expect(parseTime(formatTime(seconds))).toEqual({ ok: true, value: seconds });
    }
  });
});

describe("trimOf", () => {
  const state = (start: string, end: string) => ({ ...defaultState(["a.mp4"]), start, end });

  test("empty fields mean the whole clip", () => {
    expect(trimOf(state("", ""), 60)).toEqual({ trim: { start: null, end: null }, error: null });
  });

  test("valid ranges", () => {
    expect(trimOf(state("0:10", "0:20"), 60).trim).toEqual({ start: 10, end: 20 });
    expect(trimOf(state("", "30"), 60).trim).toEqual({ start: null, end: 30 });
  });

  test("bad text, reversed ranges and a start beyond the end are errors", () => {
    expect(trimOf(state("x", ""), 60).error).toBe("Write times like 1:30 or 90");
    expect(trimOf(state("20", "10"), 60).error).toBe("The end must come after the start");
    expect(trimOf(state("20", "20"), 60).error).toBe("The end must come after the start");
    expect(trimOf(state("90", ""), 60).error).toBe("The start is past the end of the clip");
    expect(trimOf(state("90", ""), null).error).toBeNull();
  });

  test("the clip length follows the trim", () => {
    expect(clipLength(60, { start: null, end: null })).toBe(60);
    expect(clipLength(60, { start: 10, end: 25 })).toBe(15);
    expect(clipLength(60, { start: 50, end: 500 })).toBe(10);
    expect(clipLength(null, { start: 1, end: 2 })).toBeNull();
  });
});

describe("defaultState and presets", () => {
  test("videos start on the video tab with the balanced preset", () => {
    const state = defaultState(["a.mov"]);
    expect(state.tab).toBe("video");
    expect(state.preset).toBe("balanced");
    expect(state.container).toBe("mp4");
    expect(state.replaceOriginal).toBe(false);
  });

  test("audio files start on the audio tab, away from their own format", () => {
    expect(defaultState(["a.wav"]).tab).toBe("audio");
    expect(defaultState(["a.wav"]).audioFormat).toBe("mp3");
    expect(defaultState(["a.mp3"]).audioFormat).toBe("m4a");
  });

  test("a preset sets quality, speed, height and frame rate together", () => {
    const social = applyPreset(defaultState(["a.mp4"]), "social");
    expect([social.preset, social.quality, social.maxHeight, social.fps]).toEqual(["social", 60, 1080, 30]);
    const small = applyPreset(social, "small");
    expect([small.quality, small.maxHeight]).toEqual([35, 720]);
    expect(PRESETS.map((preset) => preset.id)).toEqual(["small", "balanced", "high", "social"]);
  });

  test("only the small preset adds a name suffix", () => {
    expect(suffixFor(applyPreset(defaultState(["a.mp4"]), "small"))).toBe(" (compressed)");
    expect(suffixFor(applyPreset(defaultState(["a.mp4"]), "high"))).toBeNull();
    expect(suffixFor({ ...defaultState(["a.mp3"]), tab: "audio" })).toBeNull();
  });
});

describe("toOptions", () => {
  const none = { start: null, end: null };

  test("video", () => {
    const options = toOptions({ ...applyPreset(defaultState(["a.mp4"]), "social"), mute: true }, { start: 1, end: 2 });
    expect(options).toEqual({
      mode: "video",
      container: "mp4",
      quality: 60,
      speed: "balanced",
      maxHeight: 1080,
      fps: 30,
      mute: true,
      hardware: false,
      trim: { start: 1, end: 2 },
    });
  });

  test("audio", () => {
    const state = { ...defaultState(["a.mp3"]), tab: "audio" as const, audioFormat: "opus" as const, bitrate: 96 };
    expect(toOptions(state, none)).toEqual({ mode: "audio", format: "opus", bitrate: 96, trim: none });
  });

  test("animation", () => {
    const state = { ...defaultState(["a.mp4"]), tab: "animation" as const, width: 640, animationFps: 15, repeat: false };
    expect(toOptions(state, none)).toEqual({
      mode: "animation",
      format: "gif",
      width: 640,
      fps: 15,
      repeat: false,
      quality: 75,
      trim: none,
    });
  });
});

describe("audio size", () => {
  test("is bitrate times duration for lossy formats", () => {
    expect(audioSize(60, "mp3", 192)).toBe(1_440_000);
    expect(audioSize(10, "opus", 96)).toBe(120_000);
  });

  test("is unknown for lossless formats and unknown durations", () => {
    expect(audioSize(60, "flac", 192)).toBeNull();
    expect(audioSize(60, "wav", 192)).toBeNull();
    expect(audioSize(null, "mp3", 192)).toBeNull();
    expect(isLossless("flac") && !isLossless("mp3")).toBe(true);
    expect(BITRATES).toEqual([96, 128, 192, 256, 320]);
  });
});

describe("validity", () => {
  test("a trim error makes the dialog invalid", () => {
    const state = { ...defaultState(["a.mp4"]), start: "nope" };
    expect(isValid(state, 60)).toBe(false);
    expect(isValid(defaultState(["a.mp4"]), 60)).toBe(true);
  });

  test("numbers must be in range", () => {
    const base = defaultState(["a.mp4"]);
    expect(isValid({ ...base, quality: 101 }, 60)).toBe(false);
    expect(isValid({ ...base, tab: "audio", bitrate: 8 }, 60)).toBe(false);
    expect(isValid({ ...base, tab: "animation", width: 5 }, 60)).toBe(false);
    expect(isValid({ ...base, tab: "animation", animationFps: 61 }, 60)).toBe(false);
    expect(isValid({ ...base, tab: "animation" }, 60)).toBe(true);
  });
});

describe("warnings", () => {
  const state = defaultState(["a.mp4"]);
  const options = (tab: "video" | "audio" | "animation", change: object = {}) =>
    toOptions({ ...state, tab, ...change }, { start: null, end: null });

  test("no video or no audio in the file", () => {
    const silent = info({ audio: null });
    const mute = info({ video: null });
    expect(warningsFor({ ...state, tab: "audio" }, silent, status([]), options("audio"), 60)).toContain("This file has no audio.");
    expect(warningsFor(state, mute, status(["libx264", "aac"]), options("video"), 60)).toContain("This file has no video.");
    expect(warningsFor(state, info(), status(["libx264", "aac"]), options("video"), 60)).toEqual([]);
  });

  test("missing encoders are named", () => {
    const warnings = warningsFor(state, info(), status(["aac"]), options("video"), 60);
    expect(warnings).toEqual(["This ffmpeg can’t create that format (libx264 is missing)."]);
  });

  test("long GIFs and slow VP9 get a heads-up", () => {
    const gif = { ...state, tab: "animation" as const };
    const long = warningsFor(gif, info(), status(["gif"]), options("animation"), 40);
    expect(long.some((warning) => warning.includes("very large"))).toBe(true);
    expect(warningsFor(gif, info(), status(["gif"]), options("animation"), 10)).toEqual([]);
    const webp = { ...gif, animationFormat: "webp" as const };
    expect(warningsFor(webp, info(), status(["libwebp_anim"]), options("animation", { animationFormat: "webp" }), 40)).toEqual([]);
    const slow = { ...state, container: "webm" as const, speed: "slow" as const };
    const slowOptions = toOptions(slow, { start: null, end: null });
    expect(warningsFor(slow, info(), status(["libvpx-vp9", "libopus"]), slowOptions, 60)).toEqual(["VP9 at the slowest speed can take a long time."]);
  });
});

describe("quick convert", () => {
  test("videos offer MP4, compress, GIF and MP3", () => {
    expect(quickMediaKindsFor(["a.mov"])).toEqual(["mp4", "compress", "gif", "mp3"]);
    expect(quickMediaKindsFor(["a.mp4", "b.mkv"])).toEqual(["mp4", "compress", "gif", "mp3"]);
  });

  test("a video already in MP4 does not offer MP4 again", () => {
    expect(quickMediaKindsFor(["a.mp4", "b.MP4"])).toEqual(["compress", "gif", "mp3"]);
  });

  test("audio offers the other formats", () => {
    expect(quickMediaKindsFor(["a.wav"])).toEqual(["mp3", "m4a", "flac"]);
    expect(quickMediaKindsFor(["a.mp3"])).toEqual(["m4a", "flac", "wav"]);
    expect(quickMediaKindsFor(["a.mp3", "b.wav"])).toEqual(["mp3", "m4a", "flac", "wav"]);
  });

  test("nothing for mixes and other files", () => {
    expect(quickMediaKindsFor(["a.mp4", "b.mp3"])).toEqual([]);
    expect(quickMediaKindsFor(["a.png"])).toEqual([]);
  });

  test("labels depend on what is being converted", () => {
    expect(quickMediaLabel("mp3", "video")).toBe("Extract MP3");
    expect(quickMediaLabel("mp3", "audio")).toBe("MP3");
    expect(quickMediaLabel("mp4", "video")).toBe("MP4 (H.264)");
    expect(quickMediaLabel("compress", "video")).toBe("Compress");
  });

  test("compress shrinks and renames; the others do not rename", () => {
    const compress = quickMediaOptions("compress");
    expect(compress.suffix).toBe(" (compressed)");
    expect(compress.options).toMatchObject({ mode: "video", container: "mp4", maxHeight: 720 });
    expect(quickMediaOptions("mp4").suffix).toBeNull();
    expect(quickMediaOptions("gif").options).toMatchObject({ mode: "animation", format: "gif", width: 480 });
    expect(quickMediaOptions("flac").options).toMatchObject({ mode: "audio", format: "flac" });
    expect(quickMediaOptions("mp3").options).toMatchObject({ mode: "audio", format: "mp3", bitrate: 192 });
  });
});

describe("hardware acceleration", () => {
  test("is off by default and travels with the options", () => {
    const state = defaultState(["a.mp4"]);
    expect(state.hardware).toBe(false);
    const options = toOptions({ ...state, hardware: true }, { start: null, end: null });
    expect(options.mode === "video" && options.hardware).toBe(true);
  });

  test("quick conversions stay in software", () => {
    for (const kind of ["mp4", "compress"] as const) {
      const { options } = quickMediaOptions(kind);
      expect(options.mode === "video" && options.hardware).toBe(false);
    }
  });
});

describe("preview helpers", () => {
  test("the preview starts in the middle of what will be converted", () => {
    expect(defaultFrameTime(100, { start: null, end: null })).toBe(50);
    expect(defaultFrameTime(100, { start: 20, end: 40 })).toBe(30);
    expect(defaultFrameTime(100, { start: 90, end: null })).toBe(95);
    expect(defaultFrameTime(null, { start: null, end: null })).toBe(0);
  });

  test("the settings key changes with the file and with any setting", () => {
    const options = toOptions(defaultState(["a.mp4"]), { start: null, end: null });
    const same = toOptions(defaultState(["a.mp4"]), { start: null, end: null });
    expect(settingsKey("/a.mp4", options)).toBe(settingsKey("/a.mp4", same));
    expect(settingsKey("/b.mp4", options) === settingsKey("/a.mp4", options)).toBe(false);
    const other = toOptions({ ...defaultState(["a.mp4"]), quality: 99 }, { start: null, end: null });
    expect(settingsKey("/a.mp4", other) === settingsKey("/a.mp4", options)).toBe(false);
  });
});

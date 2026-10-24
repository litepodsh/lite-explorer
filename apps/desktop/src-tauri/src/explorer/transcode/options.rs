//! What to convert to, and the ffmpeg arguments that do it.
//!
//! Everything here is pure: options in, argument list out. ffmpeg is started without a shell, so
//! the only text that reaches a filter is numbers that `validate` has already bounded.

use std::{ffi::OsString, path::Path};

use serde::{Deserialize, Serialize};

use super::probe::MediaInfo;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Container {
    Mp4,
    Webm,
    Mov,
    Mkv,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Speed {
    Fast,
    Balanced,
    Slow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioFormat {
    Mp3,
    M4a,
    Flac,
    Wav,
    Opus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimationFormat {
    Gif,
    Webp,
}

/// A video encoder that runs on the graphics hardware. It is much faster than software and makes
/// somewhat larger files, so it is only used when the person asks for it and the machine has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HardwareEncoder {
    /// Apple's, on macOS.
    VideoToolbox,
    /// Nvidia's, on Windows and Linux.
    Nvenc,
}

impl HardwareEncoder {
    pub fn encoder(self) -> &'static str {
        match self {
            HardwareEncoder::VideoToolbox => "h264_videotoolbox",
            HardwareEncoder::Nvenc => "h264_nvenc",
        }
    }

    /// The video encoder arguments for a quality from 0 to 100. Detection runs the same ones.
    pub fn video_args(self, quality: u8) -> Vec<String> {
        match self {
            // `-q:v` goes from 1 (worst) to 100 (best).
            HardwareEncoder::VideoToolbox => vec![
                "-c:v".into(),
                self.encoder().into(),
                "-q:v".into(),
                quality.clamp(1, 100).to_string(),
            ],
            HardwareEncoder::Nvenc => vec![
                "-c:v".into(),
                self.encoder().into(),
                "-preset".into(),
                "p5".into(),
                "-rc".into(),
                "vbr".into(),
                "-cq".into(),
                crf_h264(quality).to_string(),
                "-b:v".into(),
                "0".into(),
            ],
        }
    }
}

/// A piece of the clip, in seconds. A missing side means the start or the end of the file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trim {
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TranscodeOptions {
    Video {
        container: Container,
        /// 0 to 100. Higher is better and bigger.
        quality: u8,
        speed: Speed,
        /// Larger videos are scaled down to this height; smaller ones are left alone.
        max_height: Option<u32>,
        fps: Option<u32>,
        mute: bool,
        /// Use the graphics hardware for H.264 when the machine has it. Ignored for WebM.
        #[serde(default)]
        hardware: bool,
        #[serde(default)]
        trim: Trim,
    },
    Audio {
        format: AudioFormat,
        /// kbit/s. Ignored by the lossless formats.
        bitrate: u32,
        #[serde(default)]
        trim: Trim,
    },
    Animation {
        format: AnimationFormat,
        /// Never enlarges.
        width: u32,
        fps: u32,
        repeat: bool,
        /// 0 to 100. Only WebP uses it.
        quality: u8,
        #[serde(default)]
        trim: Trim,
    },
}

/// What the source file must contain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Needs {
    Video,
    Audio,
}

impl TranscodeOptions {
    pub fn trim(&self) -> Trim {
        match self {
            TranscodeOptions::Video { trim, .. }
            | TranscodeOptions::Audio { trim, .. }
            | TranscodeOptions::Animation { trim, .. } => *trim,
        }
    }

    pub fn needs(&self) -> Needs {
        match self {
            TranscodeOptions::Audio { .. } => Needs::Audio,
            _ => Needs::Video,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            TranscodeOptions::Video { container, .. } => match container {
                Container::Mp4 => "mp4",
                Container::Webm => "webm",
                Container::Mov => "mov",
                Container::Mkv => "mkv",
            },
            TranscodeOptions::Audio { format, .. } => match format {
                AudioFormat::Mp3 => "mp3",
                AudioFormat::M4a => "m4a",
                AudioFormat::Flac => "flac",
                AudioFormat::Wav => "wav",
                AudioFormat::Opus => "opus",
            },
            TranscodeOptions::Animation { format, .. } => match format {
                AnimationFormat::Gif => "gif",
                AnimationFormat::Webp => "webp",
            },
        }
    }

    /// Whether the person asked for hardware encoding of a video.
    pub fn wants_hardware(&self) -> bool {
        matches!(self, TranscodeOptions::Video { hardware: true, container, .. } if *container != Container::Webm)
    }

    /// The encoder this needs from ffmpeg, so the UI can say what is missing.
    pub fn required_encoders(&self) -> Vec<&'static str> {
        match self {
            TranscodeOptions::Video { container, mute, .. } => {
                let (video, audio) = if *container == Container::Webm {
                    ("libvpx-vp9", "libopus")
                } else {
                    ("libx264", "aac")
                };
                if *mute {
                    vec![video]
                } else {
                    vec![video, audio]
                }
            }
            TranscodeOptions::Audio { format, .. } => vec![match format {
                AudioFormat::Mp3 => "libmp3lame",
                AudioFormat::M4a => "aac",
                AudioFormat::Flac => "flac",
                AudioFormat::Wav => "pcm_s16le",
                AudioFormat::Opus => "libopus",
            }],
            TranscodeOptions::Animation { format, .. } => vec![match format {
                AnimationFormat::Gif => "gif",
                AnimationFormat::Webp => "libwebp_anim",
            }],
        }
    }
}

fn valid_trim(trim: &Trim) -> Result<(), String> {
    for value in [trim.start, trim.end].into_iter().flatten() {
        if !value.is_finite() || value < 0.0 {
            return Err("The trim times must be positive numbers".into());
        }
    }
    if let (Some(start), Some(end)) = (trim.start, trim.end) {
        if end <= start {
            return Err("The end of the trim must come after its start".into());
        }
    }
    Ok(())
}

fn within(value: u32, min: u32, max: u32, what: &str) -> Result<(), String> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!("{what} must be between {min} and {max}"))
    }
}

pub fn validate(options: &TranscodeOptions) -> Result<(), String> {
    valid_trim(&options.trim())?;
    match options {
        TranscodeOptions::Video {
            quality,
            max_height,
            fps,
            ..
        } => {
            within(*quality as u32, 0, 100, "Quality")?;
            if let Some(height) = max_height {
                within(*height, 144, 4320, "The maximum height")?;
            }
            if let Some(fps) = fps {
                within(*fps, 1, 120, "The frame rate")?;
            }
        }
        TranscodeOptions::Audio { bitrate, .. } => within(*bitrate, 32, 512, "The bitrate")?,
        TranscodeOptions::Animation {
            width, fps, quality, ..
        } => {
            within(*width, 16, 4096, "The width")?;
            within(*fps, 1, 60, "The frame rate")?;
            within(*quality as u32, 0, 100, "Quality")?;
        }
    }
    Ok(())
}

/// x264 constant rate factor: 16 at quality 100 (near lossless to the eye) down to 36 at 0.
pub fn crf_h264(quality: u8) -> u32 {
    (36.0 - quality.min(100) as f64 * 0.2).round() as u32
}

/// VP9 constant quality: 15 at quality 100 up to 45 at 0.
pub fn crf_vp9(quality: u8) -> u32 {
    (45.0 - quality.min(100) as f64 * 0.3).round() as u32
}

/// Seconds the output will last, after trimming. `None` when the file doesn't say.
pub fn expected_duration(info: &MediaInfo, trim: &Trim) -> Option<f64> {
    let total = info.duration?;
    let start = trim.start.unwrap_or(0.0).min(total);
    let end = trim.end.unwrap_or(total).min(total);
    Some((end - start).max(0.0))
}

fn seconds(value: f64) -> String {
    format!("{value:.3}")
}

/// Even sides are required by H.264, and `-2` keeps the aspect ratio.
fn video_filters(max_height: Option<u32>, fps: Option<u32>) -> String {
    let mut filters: Vec<String> = Vec::new();
    if let Some(fps) = fps {
        filters.push(format!("fps={fps}"));
    }
    filters.push(match max_height {
        Some(height) => format!("scale=-2:'min(ih,{height})'"),
        None => "scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string(),
    });
    filters.join(",")
}

fn animation_filters(format: AnimationFormat, width: u32, fps: u32) -> String {
    let base = format!("fps={fps},scale='min(iw,{width})':-1:flags=lanczos");
    match format {
        AnimationFormat::Gif => format!(
            "{base},split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle"
        ),
        AnimationFormat::Webp => base,
    }
}

/// The same job with a different piece of the clip.
pub fn with_trim(options: &TranscodeOptions, trim: Trim) -> TranscodeOptions {
    let mut copy = options.clone();
    match &mut copy {
        TranscodeOptions::Video { trim: slot, .. }
        | TranscodeOptions::Audio { trim: slot, .. }
        | TranscodeOptions::Animation { trim: slot, .. } => *slot = trim,
    }
    copy
}

/// The full ffmpeg command line, without the program name. `hardware` is what this machine has;
/// it only matters when the options ask for it.
pub fn build_args(
    input: &Path,
    output: &Path,
    options: &TranscodeOptions,
    hardware: Option<HardwareEncoder>,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    push(&["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-progress", "pipe:1", "-nostats"]);
    let trim = options.trim();
    if let Some(start) = trim.start {
        push(&["-ss", &seconds(start)]);
    }
    if let Some(end) = trim.end {
        push(&["-to", &seconds(end)]);
    }
    args.push("-i".into());
    args.push(input.into());
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));

    match options {
        TranscodeOptions::Video {
            container,
            quality,
            speed,
            max_height,
            fps,
            mute,
            hardware: want_hardware,
            ..
        } => {
            push(&["-map", "0:v:0"]);
            if !mute {
                push(&["-map", "0:a:0?"]);
            }
            push(&["-vf", &video_filters(*max_height, *fps)]);
            if *container == Container::Webm {
                let cpu_used = match speed {
                    Speed::Fast => "5",
                    Speed::Balanced => "3",
                    Speed::Slow => "1",
                };
                push(&["-c:v", "libvpx-vp9", "-crf", &crf_vp9(*quality).to_string(), "-b:v", "0"]);
                push(&["-deadline", "good", "-cpu-used", cpu_used, "-row-mt", "1", "-pix_fmt", "yuv420p"]);
            } else if let (true, Some(encoder)) = (*want_hardware, hardware) {
                let encoder_args = encoder.video_args(*quality);
                let refs: Vec<&str> = encoder_args.iter().map(String::as_str).collect();
                push(&refs);
                push(&["-pix_fmt", "yuv420p"]);
            } else {
                let preset = match speed {
                    Speed::Fast => "veryfast",
                    Speed::Balanced => "medium",
                    Speed::Slow => "slow",
                };
                push(&["-c:v", "libx264", "-preset", preset, "-crf", &crf_h264(*quality).to_string()]);
                push(&["-pix_fmt", "yuv420p"]);
            }
            if *mute {
                push(&["-an"]);
            } else if *container == Container::Webm {
                push(&["-c:a", "libopus", "-b:a", "128k"]);
            } else {
                push(&["-c:a", "aac", "-b:a", "160k"]);
            }
            if matches!(container, Container::Mp4 | Container::Mov) {
                push(&["-movflags", "+faststart"]);
            }
        }
        TranscodeOptions::Audio { format, bitrate, .. } => {
            push(&["-map", "0:a:0", "-vn"]);
            let rate = format!("{bitrate}k");
            match format {
                AudioFormat::Mp3 => push(&["-c:a", "libmp3lame", "-b:a", &rate, "-id3v2_version", "3"]),
                AudioFormat::M4a => push(&["-c:a", "aac", "-b:a", &rate]),
                AudioFormat::Opus => push(&["-c:a", "libopus", "-b:a", &rate]),
                AudioFormat::Flac => push(&["-c:a", "flac"]),
                AudioFormat::Wav => push(&["-c:a", "pcm_s16le"]),
            }
        }
        TranscodeOptions::Animation {
            format,
            width,
            fps,
            repeat,
            quality,
            ..
        } => {
            push(&["-map", "0:v:0", "-an", "-vf", &animation_filters(*format, *width, *fps)]);
            match format {
                AnimationFormat::Gif => push(&["-loop", if *repeat { "0" } else { "-1" }]),
                AnimationFormat::Webp => push(&[
                    "-c:v",
                    "libwebp_anim",
                    "-q:v",
                    &quality.to_string(),
                    "-loop",
                    if *repeat { "0" } else { "1" },
                ]),
            }
        }
    }
    args.push(output.into());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect()
    }

    fn video(container: Container) -> TranscodeOptions {
        TranscodeOptions::Video {
            container,
            quality: 50,
            speed: Speed::Balanced,
            max_height: None,
            fps: None,
            mute: false,
            hardware: false,
            trim: Trim::default(),
        }
    }

    fn args_for(options: &TranscodeOptions) -> Vec<String> {
        strings(&build_args(Path::new("/in/clip.mov"), Path::new("/out/clip.tmp"), options, None))
    }

    /// The value that follows `flag`, or `None` when the flag is absent.
    fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter().position(|arg| arg == flag).and_then(|at| args.get(at + 1)).map(String::as_str)
    }

    fn info(duration: Option<f64>) -> MediaInfo {
        MediaInfo {
            duration,
            size: 0,
            format: String::new(),
            bit_rate: None,
            video: None,
            audio: None,
        }
    }

    #[test]
    fn quality_maps_to_a_constant_rate_factor() {
        assert_eq!((crf_h264(100), crf_h264(50), crf_h264(0)), (16, 26, 36));
        assert_eq!((crf_vp9(100), crf_vp9(50), crf_vp9(0)), (15, 30, 45));
        assert_eq!(crf_h264(250), 16, "values above 100 are clamped");
    }

    #[test]
    fn every_command_starts_with_the_safe_flags_and_ends_with_the_output() {
        for options in [
            video(Container::Mp4),
            TranscodeOptions::Audio { format: AudioFormat::Mp3, bitrate: 192, trim: Trim::default() },
            TranscodeOptions::Animation {
                format: AnimationFormat::Gif,
                width: 480,
                fps: 12,
                repeat: true,
                quality: 75,
                trim: Trim::default(),
            },
        ] {
            let args = args_for(&options);
            assert_eq!(&args[..3], ["-hide_banner", "-nostdin", "-y"], "{options:?}");
            assert_eq!(value_of(&args, "-progress"), Some("pipe:1"));
            assert_eq!(value_of(&args, "-i"), Some("/in/clip.mov"));
            assert_eq!(args.last().unwrap(), "/out/clip.tmp");
            // The input comes before any output option.
            let input_at = args.iter().position(|arg| arg == "-i").unwrap();
            assert!(args.iter().position(|arg| arg == "-map").unwrap() > input_at);
        }
    }

    #[test]
    fn mp4_uses_h264_aac_and_faststart() {
        let args = args_for(&video(Container::Mp4));
        assert_eq!(value_of(&args, "-c:v"), Some("libx264"));
        assert_eq!(value_of(&args, "-crf"), Some("26"));
        assert_eq!(value_of(&args, "-preset"), Some("medium"));
        assert_eq!(value_of(&args, "-pix_fmt"), Some("yuv420p"));
        assert_eq!(value_of(&args, "-c:a"), Some("aac"));
        assert_eq!(value_of(&args, "-movflags"), Some("+faststart"));
        assert_eq!(value_of(&args, "-vf"), Some("scale=trunc(iw/2)*2:trunc(ih/2)*2"));
        assert!(args.contains(&"0:a:0?".to_string()), "the audio track is optional");
    }

    #[test]
    fn webm_uses_vp9_and_opus() {
        let args = args_for(&video(Container::Webm));
        assert_eq!(value_of(&args, "-c:v"), Some("libvpx-vp9"));
        assert_eq!(value_of(&args, "-crf"), Some("30"));
        assert_eq!(value_of(&args, "-b:v"), Some("0"));
        assert_eq!(value_of(&args, "-cpu-used"), Some("3"));
        assert_eq!(value_of(&args, "-c:a"), Some("libopus"));
        assert!(!args.contains(&"-movflags".to_string()));
    }

    fn hardware_video(container: Container, hardware: bool) -> TranscodeOptions {
        TranscodeOptions::Video {
            container,
            quality: 70,
            speed: Speed::Slow,
            max_height: None,
            fps: None,
            mute: false,
            hardware,
            trim: Trim::default(),
        }
    }

    fn hardware_args(options: &TranscodeOptions, encoder: Option<HardwareEncoder>) -> Vec<String> {
        strings(&build_args(Path::new("/in/a.mov"), Path::new("/out/a.tmp"), options, encoder))
    }

    #[test]
    fn videotoolbox_replaces_x264_when_asked_for() {
        let args = hardware_args(&hardware_video(Container::Mp4, true), Some(HardwareEncoder::VideoToolbox));
        assert_eq!(value_of(&args, "-c:v"), Some("h264_videotoolbox"));
        assert_eq!(value_of(&args, "-q:v"), Some("70"));
        assert_eq!(value_of(&args, "-pix_fmt"), Some("yuv420p"));
        assert!(!args.contains(&"-preset".to_string()) && !args.contains(&"-crf".to_string()));
        assert_eq!(value_of(&args, "-c:a"), Some("aac"));
        assert_eq!(value_of(&args, "-movflags"), Some("+faststart"));
    }

    #[test]
    fn nvenc_uses_a_constant_quality() {
        let args = hardware_args(&hardware_video(Container::Mkv, true), Some(HardwareEncoder::Nvenc));
        assert_eq!(value_of(&args, "-c:v"), Some("h264_nvenc"));
        assert_eq!(value_of(&args, "-cq"), Some("22"));
        assert_eq!(value_of(&args, "-rc"), Some("vbr"));
        assert_eq!(value_of(&args, "-b:v"), Some("0"));
    }

    #[test]
    fn software_is_used_without_the_request_or_without_the_hardware() {
        for (wanted, available) in [(false, Some(HardwareEncoder::VideoToolbox)), (true, None), (false, None)] {
            let args = hardware_args(&hardware_video(Container::Mp4, wanted), available);
            assert_eq!(value_of(&args, "-c:v"), Some("libx264"), "wanted {wanted}, available {available:?}");
        }
    }

    #[test]
    fn webm_never_uses_the_h264_hardware() {
        let options = hardware_video(Container::Webm, true);
        assert!(!options.wants_hardware());
        let args = hardware_args(&options, Some(HardwareEncoder::VideoToolbox));
        assert_eq!(value_of(&args, "-c:v"), Some("libvpx-vp9"));
        assert!(hardware_video(Container::Mp4, true).wants_hardware());
        assert!(!hardware_video(Container::Mp4, false).wants_hardware());
    }

    #[test]
    fn videotoolbox_quality_stays_in_its_range() {
        assert_eq!(HardwareEncoder::VideoToolbox.video_args(0)[3], "1");
        assert_eq!(HardwareEncoder::VideoToolbox.video_args(100)[3], "100");
    }

    #[test]
    fn hardware_is_optional_in_json() {
        let json = r#"{"mode":"video","container":"mp4","quality":50,"speed":"fast","maxHeight":null,"fps":null,"mute":false}"#;
        let options: TranscodeOptions = serde_json::from_str(json).unwrap();
        assert!(!options.wants_hardware());
    }

    #[test]
    fn with_trim_changes_only_the_trim() {
        let original = hardware_video(Container::Mp4, true);
        let trimmed = with_trim(&original, Trim { start: Some(1.0), end: Some(2.0) });
        assert_eq!(trimmed.trim(), Trim { start: Some(1.0), end: Some(2.0) });
        assert_eq!(with_trim(&trimmed, Trim::default()), original);
    }

    #[test]
    fn mov_and_mkv_use_h264() {
        assert_eq!(value_of(&args_for(&video(Container::Mov)), "-movflags"), Some("+faststart"));
        let mkv = args_for(&video(Container::Mkv));
        assert_eq!(value_of(&mkv, "-c:v"), Some("libx264"));
        assert!(!mkv.contains(&"-movflags".to_string()));
    }

    #[test]
    fn speed_picks_the_encoder_preset() {
        let with = |speed| match video(Container::Mp4) {
            TranscodeOptions::Video { container, quality, max_height, fps, mute, trim, .. } => {
                TranscodeOptions::Video { container, quality, speed, max_height, fps, mute, hardware: false, trim }
            }
            _ => unreachable!(),
        };
        assert_eq!(value_of(&args_for(&with(Speed::Fast)), "-preset"), Some("veryfast"));
        assert_eq!(value_of(&args_for(&with(Speed::Slow)), "-preset"), Some("slow"));
    }

    #[test]
    fn resolution_and_frame_rate_go_into_one_filter_chain() {
        let options = TranscodeOptions::Video {
            container: Container::Mp4,
            quality: 80,
            speed: Speed::Fast,
            max_height: Some(720),
            fps: Some(30),
            mute: false,
            hardware: false,
            trim: Trim::default(),
        };
        let args = args_for(&options);
        assert_eq!(value_of(&args, "-vf"), Some("fps=30,scale=-2:'min(ih,720)'"));
        assert_eq!(value_of(&args, "-crf"), Some("20"));
    }

    #[test]
    fn muting_drops_the_audio() {
        let options = TranscodeOptions::Video {
            container: Container::Mp4,
            quality: 50,
            speed: Speed::Balanced,
            max_height: None,
            fps: None,
            mute: true,
            hardware: false,
            trim: Trim::default(),
        };
        let args = args_for(&options);
        assert!(args.contains(&"-an".to_string()));
        assert!(!args.contains(&"0:a:0?".to_string()));
        assert!(!args.contains(&"-c:a".to_string()));
    }

    #[test]
    fn trimming_seeks_the_input() {
        let options = TranscodeOptions::Video {
            container: Container::Mp4,
            quality: 50,
            speed: Speed::Balanced,
            max_height: None,
            fps: None,
            mute: false,
            hardware: false,
            trim: Trim { start: Some(1.5), end: Some(10.0) },
        };
        let args = args_for(&options);
        assert_eq!(value_of(&args, "-ss"), Some("1.500"));
        assert_eq!(value_of(&args, "-to"), Some("10.000"));
        let input_at = args.iter().position(|arg| arg == "-i").unwrap();
        assert!(args.iter().position(|arg| arg == "-ss").unwrap() < input_at);
        assert!(args.iter().position(|arg| arg == "-to").unwrap() < input_at);
        let only_end = TranscodeOptions::Audio {
            format: AudioFormat::Flac,
            bitrate: 128,
            trim: Trim { start: None, end: Some(3.0) },
        };
        let args = args_for(&only_end);
        assert_eq!(value_of(&args, "-ss"), None);
        assert_eq!(value_of(&args, "-to"), Some("3.000"));
    }

    #[test]
    fn audio_formats() {
        let args = |format| {
            args_for(&TranscodeOptions::Audio { format, bitrate: 192, trim: Trim::default() })
        };
        let mp3 = args(AudioFormat::Mp3);
        assert_eq!(value_of(&mp3, "-c:a"), Some("libmp3lame"));
        assert_eq!(value_of(&mp3, "-b:a"), Some("192k"));
        assert!(mp3.contains(&"-vn".to_string()));
        assert_eq!(value_of(&mp3, "-map"), Some("0:a:0"));
        assert_eq!(value_of(&args(AudioFormat::M4a), "-c:a"), Some("aac"));
        assert_eq!(value_of(&args(AudioFormat::Opus), "-c:a"), Some("libopus"));
        let flac = args(AudioFormat::Flac);
        assert_eq!(value_of(&flac, "-c:a"), Some("flac"));
        assert_eq!(value_of(&flac, "-b:a"), None, "lossless formats have no bitrate");
        assert_eq!(value_of(&args(AudioFormat::Wav), "-c:a"), Some("pcm_s16le"));
    }

    #[test]
    fn gif_uses_an_optimized_palette_and_never_enlarges() {
        let args = args_for(&TranscodeOptions::Animation {
            format: AnimationFormat::Gif,
            width: 480,
            fps: 12,
            repeat: true,
            quality: 75,
            trim: Trim::default(),
        });
        let filter = value_of(&args, "-vf").unwrap();
        assert!(filter.starts_with("fps=12,scale='min(iw,480)':-1:flags=lanczos"), "{filter}");
        assert!(filter.contains("palettegen") && filter.contains("paletteuse"), "{filter}");
        assert_eq!(value_of(&args, "-loop"), Some("0"));
        assert!(args.contains(&"-an".to_string()));
    }

    #[test]
    fn a_gif_that_does_not_repeat_says_so() {
        let args = args_for(&TranscodeOptions::Animation {
            format: AnimationFormat::Gif,
            width: 320,
            fps: 10,
            repeat: false,
            quality: 75,
            trim: Trim::default(),
        });
        assert_eq!(value_of(&args, "-loop"), Some("-1"));
    }

    #[test]
    fn animated_webp_uses_the_quality() {
        let args = args_for(&TranscodeOptions::Animation {
            format: AnimationFormat::Webp,
            width: 640,
            fps: 15,
            repeat: true,
            quality: 60,
            trim: Trim::default(),
        });
        assert_eq!(value_of(&args, "-c:v"), Some("libwebp_anim"));
        assert_eq!(value_of(&args, "-q:v"), Some("60"));
        assert_eq!(value_of(&args, "-loop"), Some("0"));
        assert!(!value_of(&args, "-vf").unwrap().contains("palettegen"));
    }

    #[test]
    fn paths_are_single_arguments_even_with_spaces_and_quotes() {
        let args = build_args(
            Path::new("/in/my \"clip\" $(rm -rf).mov"),
            Path::new("/out/it's;here.tmp"),
            &video(Container::Mp4),
            None,
        );
        let strings = strings(&args);
        assert_eq!(value_of(&strings, "-i"), Some("/in/my \"clip\" $(rm -rf).mov"));
        assert_eq!(strings.last().unwrap(), "/out/it's;here.tmp");
    }

    #[test]
    fn bad_numbers_are_rejected() {
        let mut options = video(Container::Mp4);
        assert!(validate(&options).is_ok());
        let bad = |change: &dyn Fn(&mut TranscodeOptions)| {
            let mut copy = video(Container::Mp4);
            change(&mut copy);
            validate(&copy).unwrap_err()
        };
        assert_eq!(bad(&|o| if let TranscodeOptions::Video { quality, .. } = o { *quality = 101 }), "Quality must be between 0 and 100");
        assert_eq!(bad(&|o| if let TranscodeOptions::Video { max_height, .. } = o { *max_height = Some(10) }), "The maximum height must be between 144 and 4320");
        assert_eq!(bad(&|o| if let TranscodeOptions::Video { fps, .. } = o { *fps = Some(0) }), "The frame rate must be between 1 and 120");
        if let TranscodeOptions::Video { max_height, fps, .. } = &mut options {
            *max_height = Some(1080);
            *fps = Some(60);
        }
        assert!(validate(&options).is_ok());
        assert!(validate(&TranscodeOptions::Audio { format: AudioFormat::Mp3, bitrate: 8, trim: Trim::default() }).is_err());
        assert!(validate(&TranscodeOptions::Audio { format: AudioFormat::Mp3, bitrate: 320, trim: Trim::default() }).is_ok());
        let animation = |width, fps| TranscodeOptions::Animation {
            format: AnimationFormat::Gif,
            width,
            fps,
            repeat: true,
            quality: 50,
            trim: Trim::default(),
        };
        assert!(validate(&animation(10, 10)).is_err());
        assert!(validate(&animation(480, 100)).is_err());
        assert!(validate(&animation(480, 15)).is_ok());
    }

    #[test]
    fn bad_trims_are_rejected() {
        let with = |trim| TranscodeOptions::Audio { format: AudioFormat::Mp3, bitrate: 128, trim };
        assert!(validate(&with(Trim { start: Some(-1.0), end: None })).is_err());
        assert!(validate(&with(Trim { start: Some(f64::NAN), end: None })).is_err());
        assert!(validate(&with(Trim { start: Some(5.0), end: Some(5.0) })).is_err());
        assert!(validate(&with(Trim { start: Some(5.0), end: Some(2.0) })).is_err());
        assert!(validate(&with(Trim { start: Some(1.0), end: Some(2.0) })).is_ok());
        assert!(validate(&with(Trim { start: None, end: Some(2.0) })).is_ok());
    }

    #[test]
    fn extensions_and_requirements() {
        assert_eq!(video(Container::Webm).extension(), "webm");
        assert_eq!(video(Container::Mkv).extension(), "mkv");
        assert_eq!(
            TranscodeOptions::Audio { format: AudioFormat::M4a, bitrate: 128, trim: Trim::default() }.extension(),
            "m4a"
        );
        assert_eq!(video(Container::Mp4).required_encoders(), ["libx264", "aac"]);
        assert_eq!(video(Container::Webm).required_encoders(), ["libvpx-vp9", "libopus"]);
        assert_eq!(video(Container::Mp4).needs(), Needs::Video);
        assert_eq!(
            TranscodeOptions::Audio { format: AudioFormat::Wav, bitrate: 128, trim: Trim::default() }.needs(),
            Needs::Audio
        );
    }

    #[test]
    fn the_expected_duration_follows_the_trim() {
        assert_eq!(expected_duration(&info(Some(60.0)), &Trim::default()), Some(60.0));
        assert_eq!(expected_duration(&info(Some(60.0)), &Trim { start: Some(10.0), end: Some(25.0) }), Some(15.0));
        assert_eq!(expected_duration(&info(Some(60.0)), &Trim { start: Some(50.0), end: Some(500.0) }), Some(10.0));
        assert_eq!(expected_duration(&info(Some(60.0)), &Trim { start: Some(70.0), end: None }), Some(0.0));
        assert_eq!(expected_duration(&info(None), &Trim::default()), None);
    }

    #[test]
    fn options_round_trip_through_json() {
        let options = video(Container::Webm);
        let json = serde_json::to_string(&options).unwrap();
        assert!(json.contains("\"mode\":\"video\"") && json.contains("\"maxHeight\":null"), "{json}");
        assert_eq!(serde_json::from_str::<TranscodeOptions>(&json).unwrap(), options);
        let audio: TranscodeOptions = serde_json::from_str(r#"{"mode":"audio","format":"mp3","bitrate":192}"#).unwrap();
        assert_eq!(audio.trim(), Trim::default(), "trim is optional");
    }
}

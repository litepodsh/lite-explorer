//! A single picture from a video, as it is and as it would come out of a conversion, so the
//! quality settings can be judged by eye before converting the whole file.

use std::{fs, path::Path};

use super::{
    options::{build_args, with_trim, HardwareEncoder, TranscodeOptions, Trim},
    run::{run_ffmpeg, RunError, CANCELLED},
    tools::Tools,
};

/// Both pictures are scaled to this width, so they line up exactly in the viewer.
pub const FRAME_WIDTH: u32 = 960;
/// How much of the clip is encoded to show the converted picture.
const SAMPLE_SECONDS: f64 = 1.0;

fn grab(tools: &Tools, input: &Path, at: f64, cancelled: &dyn Fn() -> bool) -> Result<Vec<u8>, String> {
    let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
    let output = scratch.path().join("frame.png");
    let args: Vec<std::ffi::OsString> = [
        "-hide_banner",
        "-nostdin",
        "-y",
        "-loglevel",
        "error",
        "-ss",
        &format!("{:.3}", at.max(0.0)),
        "-i",
    ]
    .iter()
    .map(Into::into)
    .chain([input.as_os_str().to_owned()])
    .chain(
        ["-frames:v", "1", "-vf", &format!("scale={FRAME_WIDTH}:-2:flags=bicubic")]
            .iter()
            .map(Into::into),
    )
    .chain([output.as_os_str().to_owned()])
    .collect();
    run_ffmpeg(tools, &args, None, cancelled, &mut |_| {}).map_err(|error| match error {
        RunError::Cancelled => CANCELLED.to_string(),
        RunError::Failed(message) => message,
    })?;
    fs::read(&output).map_err(|_| "ffmpeg could not read a picture at that time".to_string())
}

/// The picture at `at` seconds, straight from the file.
pub fn original_frame(tools: &Tools, input: &Path, at: f64, cancelled: &dyn Fn() -> bool) -> Result<Vec<u8>, String> {
    grab(tools, input, at, cancelled)
}

/// The same picture after converting a second of the clip with `options`.
pub fn converted_frame(
    tools: &Tools,
    input: &Path,
    options: &TranscodeOptions,
    hardware: Option<HardwareEncoder>,
    at: f64,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    if !matches!(options, TranscodeOptions::Video { .. } | TranscodeOptions::Animation { .. }) {
        return Err("Only video can be previewed".into());
    }
    let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
    let sample = scratch.path().join(format!("sample.{}", options.extension()));
    let piece = with_trim(options, Trim { start: Some(at.max(0.0)), end: Some(at.max(0.0) + SAMPLE_SECONDS) });
    let args = build_args(input, &sample, &piece, hardware);
    run_ffmpeg(tools, &args, None, cancelled, &mut |_| {}).map_err(|error| match error {
        RunError::Cancelled => CANCELLED.to_string(),
        RunError::Failed(message) => message,
    })?;
    grab(tools, &sample, 0.0, cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::transcode::{
        options::{AudioFormat, Container, Speed},
        tools::{encoders, locate, system_dirs, tools_in, Source},
    };
    use std::{path::PathBuf, process::Command};

    fn tools() -> Option<Tools> {
        let found = std::env::var_os("LITE_EXPLORER_FFMPEG_DIR")
            .and_then(|dir| tools_in(Path::new(&dir), Source::System))
            .or_else(|| locate(None, &system_dirs()));
        let Some(tools) = found else {
            eprintln!("SKIPPED: ffmpeg is not installed");
            return None;
        };
        let available = encoders(&tools);
        if !["libx264", "aac"].iter().all(|needed| available.iter().any(|have| have == needed)) {
            eprintln!("SKIPPED: this ffmpeg has no libx264 or aac");
            return None;
        }
        Some(tools)
    }

    fn make_clip(tools: &Tools, dir: &Path) -> PathBuf {
        let path = dir.join("clip.mp4");
        let status = Command::new(&tools.ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=duration=4:size=640x480:rate=25"])
            .args(["-f", "lavfi", "-i", "sine=duration=4", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac"])
            .args(["-pix_fmt", "yuv420p"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        path
    }

    fn video(quality: u8) -> TranscodeOptions {
        TranscodeOptions::Video {
            container: Container::Mp4,
            quality,
            speed: Speed::Fast,
            max_height: Some(144),
            fps: None,
            mute: false,
            hardware: false,
            trim: Trim::default(),
        }
    }

    #[test]
    fn both_pictures_have_the_same_size_and_differ() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path());
        let original = original_frame(&tools, &source, 1.5, &|| false).unwrap();
        let converted = converted_frame(&tools, &source, &video(10), None, 1.5, &|| false).unwrap();
        let size = |bytes: &[u8]| image::load_from_memory(bytes).map(|image| (image.width(), image.height())).unwrap();
        assert_eq!(size(&original), (FRAME_WIDTH, 720));
        assert_eq!(size(&original), size(&converted));
        assert_ne!(original, converted, "a 144p, low quality copy must look different");
    }

    #[test]
    fn different_times_give_different_pictures() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path());
        let early = original_frame(&tools, &source, 0.2, &|| false).unwrap();
        let late = original_frame(&tools, &source, 3.0, &|| false).unwrap();
        assert_ne!(early, late);
    }

    #[test]
    fn a_time_past_the_end_is_an_error_not_a_crash() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path());
        assert!(original_frame(&tools, &source, 500.0, &|| false).is_err());
    }

    #[test]
    fn audio_conversions_cannot_be_previewed() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path());
        let audio = TranscodeOptions::Audio { format: AudioFormat::Mp3, bitrate: 128, trim: Trim::default() };
        assert_eq!(
            converted_frame(&tools, &source, &audio, None, 0.0, &|| false).unwrap_err(),
            "Only video can be previewed"
        );
    }

    #[test]
    fn cancelling_stops_the_preview() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path());
        assert_eq!(original_frame(&tools, &source, 1.0, &|| true).unwrap_err(), CANCELLED);
    }
}

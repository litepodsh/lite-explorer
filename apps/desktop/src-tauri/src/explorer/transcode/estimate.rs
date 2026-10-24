//! Estimates how big a conversion will be by encoding a few short samples and scaling up.
//!
//! It is only an estimate: the size depends on how busy the rest of the clip is.

use std::{fs, path::Path};

use super::{
    options::{build_args, expected_duration, validate, with_trim, HardwareEncoder, TranscodeOptions, Trim},
    probe::probe,
    run::{check_source, run_ffmpeg, RunError, CANCELLED},
    tools::Tools,
};

/// Length of each sample, in seconds.
const SAMPLE_SECONDS: f64 = 3.0;
/// Where in the clip the samples are taken, as a share of its length.
const SAMPLE_POSITIONS: [f64; 3] = [0.15, 0.5, 0.85];

/// Which pieces of `start..end` to encode. A short clip is encoded whole, so its estimate is exact.
pub fn sample_ranges(start: f64, end: f64) -> Vec<(f64, f64)> {
    let length = (end - start).max(0.0);
    if length <= 0.0 {
        return Vec::new();
    }
    if length <= SAMPLE_SECONDS * SAMPLE_POSITIONS.len() as f64 {
        return vec![(start, end)];
    }
    SAMPLE_POSITIONS
        .iter()
        .map(|share| {
            let centre = start + length * share;
            let from = (centre - SAMPLE_SECONDS / 2.0).clamp(start, end - SAMPLE_SECONDS);
            (from, from + SAMPLE_SECONDS)
        })
        .collect()
}

/// The estimated size in bytes of converting `input` with `options`.
pub fn estimate(
    tools: &Tools,
    input: &Path,
    options: &TranscodeOptions,
    hardware: Option<HardwareEncoder>,
    cancelled: &dyn Fn() -> bool,
) -> Result<u64, String> {
    validate(options)?;
    let info = probe(tools, input)?;
    check_source(&info, options)?;
    let duration = info.duration.ok_or("The file doesn’t say how long it lasts")?;
    let trim = options.trim();
    let start = trim.start.unwrap_or(0.0).min(duration);
    let end = trim.end.unwrap_or(duration).min(duration);
    let length = expected_duration(&info, &trim).unwrap_or(end - start);
    let ranges = sample_ranges(start, end);
    if ranges.is_empty() {
        return Err("There is nothing to convert in that range".into());
    }

    let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
    let (mut bytes, mut seconds) = (0u64, 0.0f64);
    for (index, (from, to)) in ranges.iter().enumerate() {
        let output = scratch.path().join(format!("sample-{index}.{}", options.extension()));
        let sample = with_trim(options, Trim { start: Some(*from), end: Some(*to) });
        let args = build_args(input, &output, &sample, hardware);
        run_ffmpeg(tools, &args, Some(to - from), cancelled, &mut |_| {}).map_err(|error| match error {
            RunError::Cancelled => CANCELLED.to_string(),
            RunError::Failed(message) => message,
        })?;
        bytes += fs::metadata(&output).map_err(|error| error.to_string())?.len();
        seconds += to - from;
    }
    if seconds <= 0.0 {
        return Err("There is nothing to convert in that range".into());
    }
    Ok((bytes as f64 / seconds * length).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::transcode::{
        options::{AudioFormat, Container, Speed},
        run::run_batch,
        tools::{encoders, locate, system_dirs, tools_in, Source},
    };
    use std::{path::PathBuf, process::Command};

    #[test]
    fn a_short_clip_is_encoded_whole() {
        assert_eq!(sample_ranges(0.0, 5.0), vec![(0.0, 5.0)]);
        assert_eq!(sample_ranges(2.0, 10.5), vec![(2.0, 10.5)]);
        assert_eq!(sample_ranges(0.0, 9.0), vec![(0.0, 9.0)]);
    }

    #[test]
    fn a_long_clip_gets_three_samples_spread_out() {
        let ranges = sample_ranges(0.0, 100.0);
        assert_eq!(ranges.len(), 3);
        for (from, to) in &ranges {
            assert!((to - from - SAMPLE_SECONDS).abs() < 1e-9);
            assert!(*from >= 0.0 && *to <= 100.0);
        }
        assert!(ranges[0].1 < ranges[1].0 && ranges[1].1 < ranges[2].0, "{ranges:?}");
        assert!((ranges[1].0 - 48.5).abs() < 1e-9);
    }

    #[test]
    fn samples_stay_inside_a_trimmed_range() {
        let ranges = sample_ranges(30.0, 50.0);
        assert!(ranges.iter().all(|(from, to)| *from >= 30.0 && *to <= 50.0), "{ranges:?}");
        assert_eq!(sample_ranges(10.0, 10.0), Vec::new());
        assert_eq!(sample_ranges(10.0, 5.0), Vec::new());
    }

    fn tools() -> Option<Tools> {
        let found = std::env::var_os("LITE_EXPLORER_FFMPEG_DIR")
            .and_then(|dir| tools_in(Path::new(&dir), Source::System))
            .or_else(|| locate(None, &system_dirs()));
        let Some(tools) = found else {
            eprintln!("SKIPPED: ffmpeg is not installed");
            return None;
        };
        let available = encoders(&tools);
        for needed in ["libx264", "aac", "libmp3lame", "flac"] {
            if !available.iter().any(|have| have == needed) {
                eprintln!("SKIPPED: this ffmpeg has no {needed}");
                return None;
            }
        }
        Some(tools)
    }

    /// A clip with changing content, so its size is not trivially tiny.
    fn make_clip(tools: &Tools, dir: &Path, seconds: u32) -> PathBuf {
        let path = dir.join("clip.mp4");
        let status = Command::new(&tools.ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc2=duration={seconds}:size=640x480:rate=25"))
            .args(["-f", "lavfi", "-i"])
            .arg(format!("sine=duration={seconds}"))
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        path
    }

    fn video(quality: u8, trim: Trim) -> TranscodeOptions {
        TranscodeOptions::Video {
            container: Container::Mp4,
            quality,
            speed: Speed::Fast,
            max_height: Some(360),
            fps: None,
            mute: false,
            hardware: false,
            trim,
        }
    }

    fn actual_size(tools: &Tools, source: &Path, options: &TranscodeOptions) -> u64 {
        let outcome = run_batch(tools, &[source.to_path_buf()], options, " (real)", false, &|| false, &|_| Ok(()), &mut |_, _, _| {});
        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        fs::metadata(&outcome.converted[0].destination).unwrap().len()
    }

    #[test]
    fn a_short_clip_estimate_matches_the_real_size() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), 6);
        let options = video(50, Trim::default());
        let estimated = estimate(&tools, &source, &options, None, &|| false).unwrap() as f64;
        let real = actual_size(&tools, &source, &options) as f64;
        assert!((estimated - real).abs() / real < 0.05, "estimated {estimated}, real {real}");
    }

    #[test]
    fn a_long_clip_estimate_is_close_to_the_real_size() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), 40);
        let options = video(50, Trim::default());
        let estimated = estimate(&tools, &source, &options, None, &|| false).unwrap() as f64;
        let real = actual_size(&tools, &source, &options) as f64;
        assert!((estimated - real).abs() / real < 0.35, "estimated {estimated}, real {real}");
    }

    #[test]
    fn higher_quality_is_estimated_bigger_and_trimming_smaller() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), 20);
        let low = estimate(&tools, &source, &video(20, Trim::default()), None, &|| false).unwrap();
        let high = estimate(&tools, &source, &video(90, Trim::default()), None, &|| false).unwrap();
        assert!(low < high, "{low} !< {high}");
        let half = estimate(&tools, &source, &video(50, Trim { start: Some(0.0), end: Some(10.0) }), None, &|| false).unwrap();
        let full = estimate(&tools, &source, &video(50, Trim::default()), None, &|| false).unwrap();
        assert!(half < full, "{half} !< {full}");
        assert!((half as f64) > full as f64 * 0.3, "half {half}, full {full}");
    }

    #[test]
    fn audio_estimates_work_for_lossless_formats_too() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), 10);
        let options = TranscodeOptions::Audio { format: AudioFormat::Flac, bitrate: 128, trim: Trim::default() };
        let estimated = estimate(&tools, &source, &options, None, &|| false).unwrap() as f64;
        let real = actual_size(&tools, &source, &options) as f64;
        assert!((estimated - real).abs() / real < 0.15, "estimated {estimated}, real {real}");
    }

    #[test]
    fn cancelling_returns_the_cancelled_marker() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), 20);
        let error = estimate(&tools, &source, &video(50, Trim::default()), None, &|| true).unwrap_err();
        assert_eq!(error, CANCELLED);
    }

    #[test]
    fn bad_files_and_ranges_are_reported() {
        let Some(tools) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), 3);
        let late = video(50, Trim { start: Some(99.0), end: None });
        assert_eq!(
            estimate(&tools, &source, &late, None, &|| false).unwrap_err(),
            "The start of the trim is past the end of the clip"
        );
        let damaged = dir.path().join("damaged.mp4");
        fs::write(&damaged, "nope").unwrap();
        assert_eq!(
            estimate(&tools, &damaged, &video(50, Trim::default()), None, &|| false).unwrap_err(),
            "Unsupported or damaged media file"
        );
        assert!(fs::read_dir(dir.path()).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with("sample")));
    }
}

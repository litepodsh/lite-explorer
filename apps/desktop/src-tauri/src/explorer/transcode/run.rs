//! Runs ffmpeg for one or several files: progress, cancellation, a temporary output that is
//! checked with ffprobe before it gets its final name.

use std::{
    ffi::OsString,
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

use super::{
    options::{build_args, expected_duration, validate, HardwareEncoder, Needs, TranscodeOptions},
    probe::{probe, MediaInfo},
    progress::{fraction, parse_line, Update},
    tools::Tools,
};
use crate::explorer::convert::job::{Converted, Failure, Outcome};

pub(super) enum RunError {
    Cancelled,
    Failed(String),
}

impl From<String> for RunError {
    fn from(message: String) -> Self {
        RunError::Failed(message)
    }
}

impl From<&str> for RunError {
    fn from(message: &str) -> Self {
        RunError::Failed(message.to_string())
    }
}

pub const CANCELLED: &str = "Cancelled";

/// A line a person can act on, taken from what ffmpeg wrote to stderr.
pub fn explain(stderr: &str) -> String {
    let lower = stderr.to_lowercase();
    if lower.contains("unknown encoder") || lower.contains("encoder not found") {
        return "This ffmpeg build can’t create that format".into();
    }
    if lower.contains("permission denied") {
        return "Couldn’t write the file (permission denied)".into();
    }
    if lower.contains("no space left") {
        return "The disk is full".into();
    }
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(200).collect())
        .unwrap_or_else(|| "ffmpeg stopped unexpectedly".into())
}

/// Keeps only the end of what a stream produces, so a chatty process cannot grow memory.
fn read_tail(mut stream: impl Read) -> String {
    const KEEP: usize = 8 * 1024;
    let mut tail: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 4096];
    while let Ok(read) = stream.read(&mut buffer) {
        if read == 0 {
            break;
        }
        tail.extend_from_slice(&buffer[..read]);
        if tail.len() > KEEP {
            tail.drain(..tail.len() - KEEP);
        }
    }
    String::from_utf8_lossy(&tail).into_owned()
}

pub(super) fn run_ffmpeg(
    tools: &Tools,
    args: &[OsString],
    total: Option<f64>,
    cancelled: &dyn Fn() -> bool,
    on_fraction: &mut dyn FnMut(f64),
) -> Result<(), RunError> {
    // Already cancelled: a short job could otherwise finish before the first check below.
    if cancelled() {
        return Err(RunError::Cancelled);
    }
    let mut child = Command::new(&tools.ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| RunError::Failed(format!("Couldn’t start ffmpeg: {error}")))?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (sender, receiver) = mpsc::channel::<String>();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let errors = thread::spawn(move || read_tail(stderr));

    let mut stopped = false;
    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                if let Some(Update::Time(time)) = parse_line(&line) {
                    if let Some(done) = fraction(time, total) {
                        on_fraction(done);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            // ffmpeg closed its output: it is finishing.
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if cancelled() {
            let _ = child.kill();
            stopped = true;
            break;
        }
    }
    let status = child.wait();
    let _ = reader.join();
    let stderr_text = errors.join().unwrap_or_default();
    if stopped {
        return Err(RunError::Cancelled);
    }
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(RunError::Failed(explain(&stderr_text))),
        Err(error) => Err(RunError::Failed(error.to_string())),
    }
}

/// `stem + suffix`, then `stem + suffix + " 2"`, and so on, until the name is free.
fn unique_path(parent: &Path, stem: &str, extension: &str) -> PathBuf {
    let first = parent.join(format!("{stem}.{extension}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| parent.join(format!("{stem} {n}.{extension}")))
        .find(|candidate| !candidate.exists())
        .expect("an unused name exists")
}

pub(super) fn check_source(info: &MediaInfo, options: &TranscodeOptions) -> Result<(), String> {
    match options.needs() {
        Needs::Video if info.video.is_none() => return Err("This file has no video".into()),
        Needs::Audio if info.audio.is_none() => return Err("This file has no audio".into()),
        _ => {}
    }
    let trim = options.trim();
    if let (Some(start), Some(duration)) = (trim.start, info.duration) {
        if start >= duration {
            return Err("The start of the trim is past the end of the clip".into());
        }
    }
    Ok(())
}

/// The output must have the streams asked for and last about as long as expected.
fn check_output(info: &MediaInfo, options: &TranscodeOptions, expected: Option<f64>) -> Result<(), String> {
    let incomplete = || "The converted file looks incomplete, so it was discarded".to_string();
    match options.needs() {
        Needs::Video if info.video.is_none() => return Err(incomplete()),
        Needs::Audio if info.audio.is_none() => return Err(incomplete()),
        _ => {}
    }
    if let (Some(expected), Some(actual)) = (expected, info.duration) {
        if (actual - expected).abs() > 1.0_f64.max(expected * 0.05) {
            return Err(incomplete());
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn convert_one(
    tools: &Tools,
    source: &Path,
    options: &TranscodeOptions,
    hardware: Option<HardwareEncoder>,
    suffix: &str,
    replace_original: bool,
    cancelled: &dyn Fn() -> bool,
    remove_original: &dyn Fn(&Path) -> Result<(), String>,
    on_fraction: &mut dyn FnMut(f64),
) -> Result<PathBuf, RunError> {
    let info = probe(tools, source)?;
    check_source(&info, options)?;
    let expected = expected_duration(&info, &options.trim());

    let parent = source.parent().ok_or("Invalid local path")?;
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "media".into());
    let extension = options.extension();
    let temporary = parent.join(format!(".{stem}.{}.{extension}", uuid::Uuid::new_v4().simple()));

    let args = build_args(source, &temporary, options, hardware);
    let result = run_ffmpeg(tools, &args, expected, cancelled, on_fraction);
    let finish = || -> Result<PathBuf, RunError> {
        result?;
        let produced = probe(tools, &temporary).map_err(|_| {
            RunError::Failed("The converted file looks incomplete, so it was discarded".into())
        })?;
        check_output(&produced, options, expected)?;
        let target = unique_path(parent, &format!("{stem}{suffix}"), extension);
        fs::rename(&temporary, &target).map_err(|error| error.to_string())?;
        if replace_original {
            remove_original(source)?;
            // The original name is free now: take it instead of "name 2".
            let preferred = parent.join(format!("{stem}{suffix}.{extension}"));
            if preferred != target && !preferred.exists() && fs::rename(&target, &preferred).is_ok() {
                return Ok(preferred);
            }
        }
        Ok(target)
    };
    let outcome = finish();
    if outcome.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    outcome
}

/// Converts each file in turn, in software. See `run_batch_with`.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub fn run_batch(
    tools: &Tools,
    inputs: &[PathBuf],
    options: &TranscodeOptions,
    suffix: &str,
    replace_original: bool,
    cancelled: &dyn Fn() -> bool,
    remove_original: &dyn Fn(&Path) -> Result<(), String>,
    progress: &mut dyn FnMut(usize, usize, f64),
) -> Outcome {
    run_batch_with(tools, inputs, options, None, suffix, replace_original, cancelled, remove_original, progress)
}

/// Converts each file in turn. `hardware` is what the machine has; the options decide whether to
/// use it. `progress(index, total, fraction)` reports the file in progress.
#[allow(clippy::too_many_arguments)]
pub fn run_batch_with(
    tools: &Tools,
    inputs: &[PathBuf],
    options: &TranscodeOptions,
    hardware: Option<HardwareEncoder>,
    suffix: &str,
    replace_original: bool,
    cancelled: &dyn Fn() -> bool,
    remove_original: &dyn Fn(&Path) -> Result<(), String>,
    progress: &mut dyn FnMut(usize, usize, f64),
) -> Outcome {
    let mut outcome = Outcome::default();
    if let Err(error) = validate(options) {
        outcome.failed = inputs
            .iter()
            .map(|path| Failure {
                name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                error: error.clone(),
            })
            .collect();
        return outcome;
    }
    let total = inputs.len();
    for (index, path) in inputs.iter().enumerate() {
        if cancelled() {
            outcome.cancelled = true;
            break;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        progress(index, total, 0.0);
        let result = convert_one(
            tools,
            path,
            options,
            hardware,
            suffix,
            replace_original,
            cancelled,
            remove_original,
            &mut |done| progress(index, total, done),
        );
        match result {
            Ok(destination) => {
                outcome.converted.push(Converted {
                    source: path.to_string_lossy().into_owned(),
                    destination: destination.to_string_lossy().into_owned(),
                });
                progress(index, total, 1.0);
            }
            Err(RunError::Cancelled) => {
                outcome.cancelled = true;
                break;
            }
            Err(RunError::Failed(error)) => outcome.failed.push(Failure { name, error }),
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::transcode::{
        options::{AnimationFormat, AudioFormat, Container, Speed, Trim},
        tools::{encoders, locate, system_dirs},
    };
    use std::cell::{Cell, RefCell};

    /// What every test needs to make its source clip.
    const BASE: [&str; 2] = ["libx264", "aac"];

    /// An ffmpeg that can do what a test asks for: the folder in `LITE_EXPLORER_FFMPEG_DIR` when set
    /// (use it to point at a full build), otherwise the one on this machine. `None` skips the test,
    /// and says why, so a missing encoder never looks like a pass without being noticed.
    fn tools(extra: &[&str]) -> Option<Tools> {
        let found = std::env::var_os("LITE_EXPLORER_FFMPEG_DIR")
            .and_then(|dir| crate::explorer::transcode::tools::tools_in(Path::new(&dir), crate::explorer::transcode::tools::Source::System))
            .or_else(|| locate(None, &system_dirs()));
        let Some(tools) = found else {
            eprintln!("SKIPPED: ffmpeg is not installed");
            return None;
        };
        let available = encoders(&tools);
        for needed in BASE.iter().chain(extra) {
            if !available.iter().any(|have| have == needed) {
                eprintln!("SKIPPED: this ffmpeg has no {needed}");
                return None;
            }
        }
        Some(tools)
    }

    fn make_clip(tools: &Tools, dir: &Path, name: &str, seconds: u32, with_audio: bool) -> PathBuf {
        let path = dir.join(name);
        let mut command = Command::new(&tools.ffmpeg);
        command
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc=duration={seconds}:size=320x240:rate=25"));
        if with_audio {
            command
                .args(["-f", "lavfi", "-i"])
                .arg(format!("sine=duration={seconds}"))
                .args(["-c:a", "aac"]);
        }
        let status = command
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        path
    }

    fn video_options() -> TranscodeOptions {
        TranscodeOptions::Video {
            container: Container::Mp4,
            quality: 50,
            speed: Speed::Fast,
            max_height: Some(144),
            fps: Some(10),
            mute: false,
            hardware: false,
            trim: Trim::default(),
        }
    }

    fn convert(tools: &Tools, input: &Path, options: &TranscodeOptions, suffix: &str) -> Outcome {
        run_batch(tools, &[input.to_path_buf()], options, suffix, false, &|| false, &|_| Ok(()), &mut |_, _, _| {})
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|entry| {
                let name = entry.unwrap().file_name().to_string_lossy().into_owned();
                name.starts_with('.').then_some(name)
            })
            .collect()
    }

    #[test]
    fn errors_are_turned_into_plain_sentences() {
        assert_eq!(explain("Unknown encoder 'libx264'"), "This ffmpeg build can’t create that format");
        assert_eq!(explain("x: Permission denied"), "Couldn’t write the file (permission denied)");
        assert_eq!(explain("av_write_frame(): No space left on device"), "The disk is full");
        assert_eq!(explain("first\n\nlast useful line\n  \n"), "last useful line");
        assert_eq!(explain(""), "ffmpeg stopped unexpectedly");
        assert_eq!(explain(&"x".repeat(500)).chars().count(), 200);
    }

    #[test]
    fn names_never_overwrite_anything() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(unique_path(dir.path(), "clip", "mp4"), dir.path().join("clip.mp4"));
        fs::write(dir.path().join("clip.mp4"), "x").unwrap();
        assert_eq!(unique_path(dir.path(), "clip", "mp4"), dir.path().join("clip 2.mp4"));
        fs::write(dir.path().join("clip 2.mp4"), "x").unwrap();
        assert_eq!(unique_path(dir.path(), "clip", "mp4"), dir.path().join("clip 3.mp4"));
    }

    #[test]
    fn a_long_stderr_keeps_only_its_end() {
        let text = "a".repeat(20_000) + "THE END";
        let tail = read_tail(text.as_bytes());
        assert_eq!(tail.len(), 8 * 1024);
        assert!(tail.ends_with("THE END"));
    }

    #[test]
    fn compresses_a_video_next_to_the_original() {
        let Some(tools) = tools(&[]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "clip.mp4", 2, true);
        let outcome = convert(&tools, &source, &video_options(), " (compressed)");
        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        let destination = PathBuf::from(&outcome.converted[0].destination);
        assert_eq!(destination, dir.path().join("clip (compressed).mp4"));
        assert!(source.exists());
        let info = probe(&tools, &destination).unwrap();
        let video = info.video.unwrap();
        assert_eq!((video.codec.as_str(), video.height), ("h264", 144));
        assert!((video.fps.unwrap() - 10.0).abs() < 0.1);
        assert!(info.audio.is_some());
        assert!((info.duration.unwrap() - 2.0).abs() < 0.3);
        assert!(leftovers(dir.path()).is_empty(), "{:?}", leftovers(dir.path()));
    }

    #[test]
    fn hardware_encoding_makes_a_playable_file_when_the_machine_has_it() {
        let Some(tools) = tools(&[]) else { return };
        let Some(encoder) = crate::explorer::transcode::tools::detect_hardware(&tools) else {
            eprintln!("SKIPPED: no hardware encoder on this machine");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "clip.mp4", 2, true);
        let options = TranscodeOptions::Video {
            container: Container::Mp4,
            quality: 60,
            speed: Speed::Fast,
            max_height: Some(144),
            fps: None,
            mute: false,
            hardware: true,
            trim: Trim::default(),
        };
        let outcome = run_batch_with(
            &tools,
            std::slice::from_ref(&source),
            &options,
            Some(encoder),
            " (hw)",
            false,
            &|| false,
            &|_| Ok(()),
            &mut |_, _, _| {},
        );
        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        let info = probe(&tools, &PathBuf::from(&outcome.converted[0].destination)).unwrap();
        assert_eq!((info.video.unwrap().codec.as_str(), info.audio.is_some()), ("h264", true));
    }

    #[test]
    fn webm_can_be_silent() {
        let Some(tools) = tools(&["libvpx-vp9", "libopus"]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "clip.mp4", 1, true);
        let options = TranscodeOptions::Video {
            container: Container::Webm,
            quality: 60,
            speed: Speed::Fast,
            max_height: Some(144),
            fps: None,
            mute: true,
            hardware: false,
            trim: Trim::default(),
        };
        let outcome = convert(&tools, &source, &options, "");
        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        let info = probe(&tools, &PathBuf::from(&outcome.converted[0].destination)).unwrap();
        assert_eq!(info.video.unwrap().codec, "vp9");
        assert!(info.audio.is_none());
    }

    #[test]
    fn extracts_and_converts_audio() {
        let Some(tools) = tools(&["libmp3lame", "flac", "libopus"]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "clip.mp4", 2, true);
        for (format, extension, codec) in [
            (AudioFormat::Mp3, "mp3", "mp3"),
            (AudioFormat::Flac, "flac", "flac"),
            (AudioFormat::Wav, "wav", "pcm_s16le"),
            (AudioFormat::Opus, "opus", "opus"),
            (AudioFormat::M4a, "m4a", "aac"),
        ] {
            let options = TranscodeOptions::Audio { format, bitrate: 128, trim: Trim::default() };
            let outcome = convert(&tools, &source, &options, "");
            assert!(outcome.failed.is_empty(), "{extension}: {:?}", outcome.failed);
            let destination = PathBuf::from(&outcome.converted[0].destination);
            assert_eq!(destination, dir.path().join(format!("clip.{extension}")));
            let info = probe(&tools, &destination).unwrap();
            assert!(info.video.is_none(), "{extension} should have no video");
            assert_eq!(info.audio.unwrap().codec, codec, "{extension}");
        }
    }

    #[test]
    fn makes_animations() {
        let Some(tools) = tools(&["gif", "libwebp_anim"]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "clip.mp4", 2, false);
        for (format, extension, codec) in [(AnimationFormat::Gif, "gif", "gif"), (AnimationFormat::Webp, "webp", "webp")] {
            let options = TranscodeOptions::Animation {
                format,
                width: 160,
                fps: 10,
                repeat: true,
                quality: 60,
                trim: Trim::default(),
            };
            let outcome = convert(&tools, &source, &options, "");
            assert!(outcome.failed.is_empty(), "{extension}: {:?}", outcome.failed);
            let info = probe(&tools, &PathBuf::from(&outcome.converted[0].destination)).unwrap();
            let video = info.video.unwrap();
            // ffprobe names the animated WebP codec "webp_anim" in some builds.
            assert!(video.codec.starts_with(codec), "{extension}: {}", video.codec);
            assert_eq!(video.width, 160, "{extension}");
        }
    }

    #[test]
    fn trims_the_clip() {
        let Some(tools) = tools(&[]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "clip.mp4", 3, true);
        let options = TranscodeOptions::Audio {
            format: AudioFormat::Wav,
            bitrate: 128,
            trim: Trim { start: Some(0.5), end: Some(1.5) },
        };
        let outcome = convert(&tools, &source, &options, "");
        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        let info = probe(&tools, &PathBuf::from(&outcome.converted[0].destination)).unwrap();
        assert!((info.duration.unwrap() - 1.0).abs() < 0.15, "{:?}", info.duration);
    }

    #[test]
    fn the_second_run_does_not_overwrite_the_first() {
        let Some(tools) = tools(&[]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "clip.mp4", 1, true);
        let first = convert(&tools, &source, &video_options(), "");
        let second = convert(&tools, &source, &video_options(), "");
        assert_eq!(PathBuf::from(&first.converted[0].destination), dir.path().join("clip 2.mp4"));
        assert_eq!(PathBuf::from(&second.converted[0].destination), dir.path().join("clip 3.mp4"));
    }

    #[test]
    fn replacing_the_original_keeps_its_name() {
        let Some(tools) = tools(&[]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "clip.mp4", 1, true);
        let original_size = fs::metadata(&source).unwrap().len();
        let removed = RefCell::new(Vec::new());
        let outcome = run_batch(
            &tools,
            std::slice::from_ref(&source),
            &video_options(),
            "",
            true,
            &|| false,
            &|path| {
                removed.borrow_mut().push(path.to_path_buf());
                fs::remove_file(path).map_err(|error| error.to_string())
            },
            &mut |_, _, _| {},
        );
        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        assert_eq!(removed.into_inner(), std::slice::from_ref(&source));
        assert_eq!(PathBuf::from(&outcome.converted[0].destination), source);
        assert_eq!(probe(&tools, &source).unwrap().video.unwrap().height, 144);
        assert_ne!(fs::metadata(&source).unwrap().len(), original_size);
    }

    #[test]
    fn a_failed_check_keeps_the_original() {
        let Some(tools) = tools(&[]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "clip.mp4", 1, true);
        let outcome = run_batch(
            &tools,
            std::slice::from_ref(&source),
            &video_options(),
            "",
            true,
            &|| false,
            &|_| Err("the trash is unavailable".to_string()),
            &mut |_, _, _| {},
        );
        assert_eq!(outcome.failed.len(), 1);
        assert!(source.exists(), "the original must not be lost");
    }

    #[test]
    fn reports_progress_per_file_and_reaches_the_end() {
        let Some(tools) = tools(&[]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let a = make_clip(&tools, dir.path(), "a.mp4", 1, true);
        let b = make_clip(&tools, dir.path(), "b.mp4", 1, true);
        let seen = RefCell::new(Vec::new());
        run_batch(&tools, &[a, b], &video_options(), " (x)", false, &|| false, &|_| Ok(()), &mut |index, total, done| {
            seen.borrow_mut().push((index, total, done));
        });
        let seen = seen.into_inner();
        assert_eq!(seen.first(), Some(&(0, 2, 0.0)));
        assert!(seen.contains(&(0, 2, 1.0)));
        assert_eq!(seen.last(), Some(&(1, 2, 1.0)));
        assert!(seen.iter().all(|(index, _, done)| *index < 2 && (0.0..=1.0).contains(done)));
    }

    #[test]
    fn cancelling_stops_ffmpeg_and_leaves_no_files() {
        let Some(tools) = tools(&[]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "long.mp4", 60, true);
        let options = TranscodeOptions::Video {
            container: Container::Mp4,
            quality: 100,
            speed: Speed::Slow,
            max_height: None,
            fps: None,
            mute: false,
            hardware: false,
            trim: Trim::default(),
        };
        let reports = Cell::new(0);
        let started = std::time::Instant::now();
        let outcome = run_batch(
            &tools,
            std::slice::from_ref(&source),
            &options,
            "",
            false,
            &|| reports.get() >= 2,
            &|_| Ok(()),
            &mut |_, _, _| reports.set(reports.get() + 1),
        );
        assert!(outcome.cancelled);
        assert!(outcome.converted.is_empty() && outcome.failed.is_empty());
        assert!(started.elapsed() < Duration::from_secs(30), "{:?}", started.elapsed());
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["long.mp4"], "no output and no temporary file");
    }

    #[test]
    fn files_that_do_not_fit_the_job_fail_with_a_reason() {
        let Some(tools) = tools(&[]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let silent = make_clip(&tools, dir.path(), "silent.mp4", 1, false);
        let damaged = dir.path().join("damaged.mp4");
        fs::write(&damaged, "this is not a video").unwrap();
        let audio = TranscodeOptions::Audio { format: AudioFormat::Mp3, bitrate: 128, trim: Trim::default() };
        let outcome = convert(&tools, &silent, &audio, "");
        assert_eq!(outcome.failed[0].error, "This file has no audio");
        let outcome = convert(&tools, &damaged, &video_options(), "");
        assert_eq!(outcome.failed[0].error, "Unsupported or damaged media file");
        let late = TranscodeOptions::Audio {
            format: AudioFormat::Mp3,
            bitrate: 128,
            trim: Trim { start: Some(99.0), end: None },
        };
        let with_sound = make_clip(&tools, dir.path(), "sound.mp4", 1, true);
        let outcome = convert(&tools, &with_sound, &late, "");
        assert_eq!(outcome.failed[0].error, "The start of the trim is past the end of the clip");
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn a_failing_file_does_not_stop_the_others() {
        let Some(tools) = tools(&[]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let damaged = dir.path().join("damaged.mp4");
        fs::write(&damaged, "nope").unwrap();
        let good = make_clip(&tools, dir.path(), "good.mp4", 1, true);
        let outcome = run_batch(&tools, &[damaged, good], &video_options(), " (x)", false, &|| false, &|_| Ok(()), &mut |_, _, _| {});
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.converted.len(), 1);
    }

    #[test]
    fn invalid_options_fail_every_file_before_running_ffmpeg() {
        let Some(tools) = tools(&[]) else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = make_clip(&tools, dir.path(), "clip.mp4", 1, true);
        let options = TranscodeOptions::Video {
            container: Container::Mp4,
            quality: 200,
            speed: Speed::Fast,
            max_height: None,
            fps: None,
            mute: false,
            hardware: false,
            trim: Trim::default(),
        };
        let outcome = convert(&tools, &source, &options, "");
        assert_eq!(outcome.failed[0].error, "Quality must be between 0 and 100");
        assert!(outcome.converted.is_empty());
    }
}

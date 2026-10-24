//! Reads a media file's duration and streams with ffprobe.

use std::{
    path::Path,
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

use super::tools::Tools;

#[derive(Debug, Deserialize)]
struct Raw {
    #[serde(default)]
    format: RawFormat,
    #[serde(default)]
    streams: Vec<RawStream>,
}

#[derive(Debug, Default, Deserialize)]
struct RawFormat {
    duration: Option<String>,
    size: Option<String>,
    format_name: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawDisposition {
    #[serde(default)]
    attached_pic: u8,
}

#[derive(Debug, Deserialize)]
struct RawStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    bit_rate: Option<String>,
    channels: Option<u32>,
    sample_rate: Option<String>,
    duration: Option<String>,
    #[serde(default)]
    disposition: RawDisposition,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoStream {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub bit_rate: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStream {
    pub codec: String,
    pub channels: u32,
    pub sample_rate: Option<u32>,
    pub bit_rate: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    /// Seconds. Missing for some live or broken files.
    pub duration: Option<f64>,
    pub size: u64,
    pub format: String,
    pub bit_rate: Option<u64>,
    pub video: Option<VideoStream>,
    pub audio: Option<AudioStream>,
}

/// "30000/1001" gives 29.97. "0/0" and zero rates give nothing.
pub fn parse_frame_rate(text: &str) -> Option<f64> {
    let (numerator, denominator) = text.split_once('/')?;
    let numerator: f64 = numerator.parse().ok()?;
    let denominator: f64 = denominator.parse().ok()?;
    (numerator > 0.0 && denominator > 0.0).then(|| numerator / denominator)
}

fn number<T: std::str::FromStr>(text: &Option<String>) -> Option<T> {
    text.as_deref().and_then(|text| text.parse().ok())
}

pub fn parse(json: &str) -> Result<MediaInfo, String> {
    let raw: Raw = serde_json::from_str(json).map_err(|_| "Unsupported or damaged media file".to_string())?;
    // Cover art in an MP3 shows up as a one-frame video stream; it is not a video.
    let video = raw.streams.iter().find(|stream| {
        stream.codec_type.as_deref() == Some("video") && stream.disposition.attached_pic == 0
    });
    let audio = raw
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"));
    if video.is_none() && audio.is_none() {
        return Err("This file has no audio or video".into());
    }
    let duration = number::<f64>(&raw.format.duration)
        .or_else(|| video.and_then(|stream| number(&stream.duration)))
        .or_else(|| audio.and_then(|stream| number(&stream.duration)))
        .filter(|seconds| *seconds >= 0.0);
    Ok(MediaInfo {
        duration,
        size: number(&raw.format.size).unwrap_or(0),
        format: raw.format.format_name.unwrap_or_default(),
        bit_rate: number(&raw.format.bit_rate),
        video: video.map(|stream| VideoStream {
            codec: stream.codec_name.clone().unwrap_or_default(),
            width: stream.width.unwrap_or(0),
            height: stream.height.unwrap_or(0),
            fps: stream
                .avg_frame_rate
                .as_deref()
                .and_then(parse_frame_rate)
                .or_else(|| stream.r_frame_rate.as_deref().and_then(parse_frame_rate)),
            bit_rate: number(&stream.bit_rate),
        }),
        audio: audio.map(|stream| AudioStream {
            codec: stream.codec_name.clone().unwrap_or_default(),
            channels: stream.channels.unwrap_or(0),
            sample_rate: number(&stream.sample_rate),
            bit_rate: number(&stream.bit_rate),
        }),
    })
}

/// Runs ffprobe on an absolute path.
pub fn probe(tools: &Tools, path: &Path) -> Result<MediaInfo, String> {
    let output = Command::new(&tools.ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams", "-i"])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("Unsupported or damaged media file".into());
    }
    parse(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIDEO_AND_AUDIO: &str = r#"{
      "streams": [
        {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,
         "avg_frame_rate":"30000/1001","r_frame_rate":"30000/1001","bit_rate":"4500000",
         "disposition":{"attached_pic":0}},
        {"codec_type":"audio","codec_name":"aac","channels":2,"sample_rate":"48000","bit_rate":"128000"}
      ],
      "format": {"duration":"61.500000","size":"35000000","format_name":"mov,mp4,m4a,3gp,3g2,mj2","bit_rate":"4700000"}
    }"#;

    #[test]
    fn reads_a_video_with_sound() {
        let info = parse(VIDEO_AND_AUDIO).unwrap();
        assert_eq!(info.duration, Some(61.5));
        assert_eq!(info.size, 35_000_000);
        assert_eq!(info.format, "mov,mp4,m4a,3gp,3g2,mj2");
        assert_eq!(info.bit_rate, Some(4_700_000));
        let video = info.video.unwrap();
        assert_eq!((video.codec.as_str(), video.width, video.height), ("h264", 1920, 1080));
        assert!((video.fps.unwrap() - 29.97).abs() < 0.01);
        assert_eq!(video.bit_rate, Some(4_500_000));
        let audio = info.audio.unwrap();
        assert_eq!((audio.codec.as_str(), audio.channels, audio.sample_rate), ("aac", 2, Some(48_000)));
    }

    #[test]
    fn reads_an_audio_file() {
        let json = r#"{"streams":[{"codec_type":"audio","codec_name":"flac","channels":2,"sample_rate":"44100"}],
                       "format":{"duration":"200.0","size":"30000000","format_name":"flac"}}"#;
        let info = parse(json).unwrap();
        assert!(info.video.is_none());
        assert_eq!(info.audio.unwrap().codec, "flac");
        assert_eq!(info.duration, Some(200.0));
    }

    #[test]
    fn cover_art_is_not_a_video() {
        let json = r#"{"streams":[
            {"codec_type":"audio","codec_name":"mp3","channels":2,"sample_rate":"44100"},
            {"codec_type":"video","codec_name":"mjpeg","width":500,"height":500,"disposition":{"attached_pic":1}}],
            "format":{"duration":"10","size":"1000","format_name":"mp3"}}"#;
        let info = parse(json).unwrap();
        assert!(info.video.is_none());
        assert!(info.audio.is_some());
    }

    #[test]
    fn the_duration_can_come_from_a_stream() {
        let json = r#"{"streams":[{"codec_type":"video","codec_name":"vp9","width":640,"height":360,"duration":"12.5"}],
                       "format":{"size":"10","format_name":"matroska,webm"}}"#;
        assert_eq!(parse(json).unwrap().duration, Some(12.5));
    }

    #[test]
    fn a_missing_duration_stays_unknown() {
        let json = r#"{"streams":[{"codec_type":"video","codec_name":"vp9","width":640,"height":360}],"format":{}}"#;
        let info = parse(json).unwrap();
        assert_eq!(info.duration, None);
        assert_eq!(info.size, 0);
    }

    #[test]
    fn files_without_audio_or_video_are_rejected() {
        let json = r#"{"streams":[{"codec_type":"subtitle","codec_name":"srt"}],"format":{"format_name":"srt"}}"#;
        assert_eq!(parse(json).unwrap_err(), "This file has no audio or video");
        assert_eq!(parse(r#"{"streams":[],"format":{}}"#).unwrap_err(), "This file has no audio or video");
    }

    #[test]
    fn garbage_is_a_clean_error() {
        assert_eq!(parse("not json").unwrap_err(), "Unsupported or damaged media file");
        assert_eq!(parse("").unwrap_err(), "Unsupported or damaged media file");
    }

    #[test]
    fn frame_rates() {
        assert_eq!(parse_frame_rate("25/1"), Some(25.0));
        assert!((parse_frame_rate("24000/1001").unwrap() - 23.976).abs() < 0.001);
        assert_eq!(parse_frame_rate("0/0"), None);
        assert_eq!(parse_frame_rate("30/0"), None);
        assert_eq!(parse_frame_rate("30"), None);
        assert_eq!(parse_frame_rate("abc/def"), None);
    }

    /// Against the ffmpeg installed on this machine, when there is one.
    #[test]
    fn probes_a_real_generated_clip() {
        use crate::explorer::transcode::tools::{encoders, locate, system_dirs};
        let Some(tools) = locate(None, &system_dirs()) else {
            eprintln!("skipped: ffmpeg is not installed");
            return;
        };
        if !encoders(&tools).iter().any(|name| name == "libx264") || !encoders(&tools).iter().any(|name| name == "aac") {
            eprintln!("skipped: this ffmpeg has no libx264 or aac");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("clip.mp4");
        let status = Command::new(&tools.ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=25"])
            .args(["-f", "lavfi", "-i", "sine=duration=2", "-c:v", "libx264", "-c:a", "aac", "-shortest"])
            .arg(&clip)
            .status()
            .unwrap();
        assert!(status.success());
        let info = probe(&tools, &clip).unwrap();
        let video = info.video.unwrap();
        assert_eq!((video.width, video.height, video.codec.as_str()), (320, 240, "h264"));
        assert!((video.fps.unwrap() - 25.0).abs() < 0.01);
        assert!((info.duration.unwrap() - 2.0).abs() < 0.2, "{:?}", info.duration);
        assert_eq!(info.audio.unwrap().codec, "aac");
        assert!(probe(&tools, &dir.path().join("missing.mp4")).is_err());
    }
}

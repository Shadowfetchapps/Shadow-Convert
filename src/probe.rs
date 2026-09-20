use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::detect::{self, MediaKind};
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct Probe {
    pub path: PathBuf,
    pub kind: MediaKind,
    pub format_name: Option<String>,
    pub duration_secs: Option<f64>,
    pub size_bytes: u64,
    pub bit_rate: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub has_video: bool,
    pub has_audio: bool,
    pub rotation: Option<i32>,
    pub raw_json: String,
}

impl Probe {
    pub fn resolution_label(&self) -> Option<String> {
        match (self.width, self.height) {
            (Some(w), Some(h)) => Some(format!("{w}×{h}")),
            _ => None,
        }
    }

    pub fn duration_label(&self) -> Option<String> {
        self.duration_secs.map(format_duration)
    }

    pub fn size_label(&self) -> String {
        format_bytes(self.size_bytes)
    }

    pub fn summary_lines(&self) -> Vec<(String, String)> {
        let mut rows = Vec::new();
        rows.push(("Type".into(), self.kind.as_label().into()));
        if let Some(fmt) = &self.format_name {
            rows.push(("Container".into(), fmt.clone()));
        }
        if let Some(d) = self.duration_label() {
            rows.push(("Duration".into(), d));
        }
        if let Some(res) = self.resolution_label() {
            rows.push(("Resolution".into(), res));
        }
        if let Some(fps) = self.fps {
            rows.push(("Frame rate".into(), format!("{fps:.2} fps")));
        }
        if let Some(c) = &self.video_codec {
            rows.push(("Video".into(), c.clone()));
        }
        if let Some(c) = &self.audio_codec {
            rows.push(("Audio".into(), c.clone()));
        }
        if let Some(sr) = self.sample_rate {
            rows.push(("Sample rate".into(), format!("{sr} Hz")));
        }
        rows.push(("Size".into(), self.size_label()));
        if let Some(br) = self.bit_rate {
            rows.push(("Bitrate".into(), format!("{} kbps", br / 1000)));
        }
        rows
    }
}

pub fn probe(path: &Path) -> Result<Probe> {
    if !path.exists() {
        return Err(Error::user(format!(
            "The file {} does not exist.",
            path.display()
        )));
    }
    if path.is_dir() {
        return Err(Error::user(
            "That is a folder. Drop a video, audio, or image file instead.",
        ));
    }
    let meta = std::fs::metadata(path)?;
    if meta.len() == 0 {
        return Err(Error::user(
            "This file is empty (0 bytes), so there is nothing to convert.",
        ));
    }

    let sniffed = detect::sniff_kind(path);
    let ffprobe = which::which("ffprobe").map_err(|_| {
        Error::user("FFprobe is not installed. Install the ffmpeg package to convert media.")
    })?;

    let output = Command::new(ffprobe)
        .arg("-v")
        .arg("error")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg("-show_entries")
        .arg("stream_tags=rotate:format_tags:stream_disposition")
        .arg(path)
        .output()
        .map_err(|err| Error::detailed("Could not start ffprobe.", err.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if sniffed == MediaKind::Unknown {
            return Err(Error::detailed(
                "This file is not a supported video, audio, or image.",
                stderr,
            ));
        }
        return Err(Error::detailed(
            "Could not read this file. It may be damaged or an unusual format.",
            stderr,
        ));
    }

    let raw = String::from_utf8_lossy(&output.stdout).into_owned();
    let value: Value = serde_json::from_str(&raw).map_err(|err| {
        Error::detailed("ffprobe returned unreadable metadata.", err.to_string())
    })?;

    let mut probe = Probe {
        path: path.to_path_buf(),
        kind: sniffed,
        format_name: None,
        duration_secs: None,
        size_bytes: meta.len(),
        bit_rate: None,
        width: None,
        height: None,
        fps: None,
        video_codec: None,
        audio_codec: None,
        sample_rate: None,
        channels: None,
        has_video: false,
        has_audio: false,
        rotation: None,
        raw_json: raw,
    };

    if let Some(format) = value.get("format") {
        probe.format_name = format
            .get("format_name")
            .and_then(|v| v.as_str())
            .map(|s| s.split(',').next().unwrap_or(s).to_string());
        probe.duration_secs = format
            .get("duration")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok());
        probe.bit_rate = format
            .get("bit_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok());
        if let Some(size) = format
            .get("size")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            probe.size_bytes = size;
        }
    }

    if let Some(streams) = value.get("streams").and_then(|v| v.as_array()) {
        for stream in streams {
            let codec_type = stream.get("codec_type").and_then(|v| v.as_str());
            let codec = stream
                .get("codec_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            match codec_type {
                Some("video") => {
                    let is_cover = stream
                        .get("disposition")
                        .and_then(|d| d.get("attached_pic"))
                        .and_then(|v| v.as_i64())
                        == Some(1);
                    if is_cover && probe.has_video {
                        continue;
                    }
                    if !is_cover {
                        probe.has_video = true;
                        probe.video_codec = codec;
                        probe.width = stream
                            .get("width")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32);
                        probe.height = stream
                            .get("height")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32);
                        probe.fps = parse_rate(stream.get("avg_frame_rate"))
                            .or_else(|| parse_rate(stream.get("r_frame_rate")));
                        if let Some(tags) = stream.get("tags") {
                            if let Some(rot) = tags
                                .get("rotate")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse().ok())
                            {
                                probe.rotation = Some(rot);
                            }
                        }
                    } else if probe.video_codec.is_none() {
                        probe.video_codec = codec;
                    }
                }
                Some("audio") => {
                    probe.has_audio = true;
                    probe.audio_codec = codec;
                    probe.sample_rate = stream
                        .get("sample_rate")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok());
                    probe.channels = stream
                        .get("channels")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32);
                }
                _ => {}
            }
        }
    }

    probe.kind = classify(&probe, sniffed);
    if probe.kind == MediaKind::Unknown {
        return Err(Error::user(
            "This file is not a supported video, audio, or image.",
        ));
    }
    Ok(probe)
}

fn classify(probe: &Probe, sniffed: MediaKind) -> MediaKind {
    if sniffed == MediaKind::Image {
        return MediaKind::Image;
    }
    let still = match probe.duration_secs {
        Some(d) => d <= 0.2,
        None => true,
    };
    if probe.has_video && !still {
        return MediaKind::Video;
    }
    if probe.has_video && !probe.has_audio && still {
        return MediaKind::Image;
    }
    if probe.has_audio && !probe.has_video {
        return MediaKind::Audio;
    }
    if sniffed != MediaKind::Unknown {
        return sniffed;
    }
    if probe.has_audio {
        MediaKind::Audio
    } else if probe.has_video && !still {
        MediaKind::Video
    } else if probe.has_video {
        MediaKind::Image
    } else {
        MediaKind::Unknown
    }
}

fn parse_rate(value: Option<&Value>) -> Option<f64> {
    let s = value?.as_str()?;
    if s == "0/0" || s == "N/A" {
        return None;
    }
    if let Some((n, d)) = s.split_once('/') {
        let n: f64 = n.parse().ok()?;
        let d: f64 = d.parse().ok()?;
        if d == 0.0 {
            return None;
        }
        return Some(n / d);
    }
    s.parse().ok()
}

pub fn format_duration(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "—".into();
    }
    let total = secs.round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let n = bytes as f64;
    if n >= GB {
        format!("{:.2} GB", n / GB)
    } else if n >= MB {
        format!("{:.1} MB", n / MB)
    } else if n >= KB {
        format!("{:.0} KB", n / KB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_format() {
        assert_eq!(format_duration(65.0), "1:05");
        assert_eq!(format_duration(3661.0), "1:01:01");
    }

    #[test]
    fn bytes_format() {
        assert_eq!(format_bytes(500), "500 B");
        assert!(format_bytes(2 * 1024 * 1024).contains("MB"));
    }
}

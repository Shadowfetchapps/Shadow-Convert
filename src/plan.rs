use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::detect::MediaKind;
use crate::error::{Error, Result};
use crate::hw::{self, HardwareCaps};
use crate::paths;
use crate::probe::Probe;
use crate::settings::Quality;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoContainer {
    Mp4,
    Mkv,
    Webm,
}

impl VideoContainer {
    pub fn ext(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::Webm => "webm",
        }
    }

    pub fn from_ext(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "mp4" | "m4v" => Some(Self::Mp4),
            "mkv" => Some(Self::Mkv),
            "webm" => Some(Self::Webm),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Mp3,
    Wav,
    Flac,
    Aac,
    Opus,
}

impl AudioFormat {
    pub fn ext(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Aac => "m4a",
            Self::Opus => "opus",
        }
    }

    pub fn from_ext(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "mp3" => Some(Self::Mp3),
            "wav" => Some(Self::Wav),
            "flac" => Some(Self::Flac),
            "aac" | "m4a" => Some(Self::Aac),
            "opus" | "ogg" => Some(Self::Opus),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Avif,
}

impl ImageFormat {
    pub fn ext(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Avif => "avif",
        }
    }

    pub fn from_ext(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            "avif" => Some(Self::Avif),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionTarget {
    Original,
    P1080,
    P1440,
    P2160,
}

impl ResolutionTarget {
    pub fn height(self) -> Option<u32> {
        match self {
            Self::Original => None,
            Self::P1080 => Some(1080),
            Self::P1440 => Some(1440),
            Self::P2160 => Some(2160),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::P1080 => "1080p",
            Self::P1440 => "1440p",
            Self::P2160 => "4K",
        }
    }
}

#[derive(Debug, Clone)]
pub enum JobKind {
    Video {
        container: VideoContainer,
        resolution: ResolutionTarget,
        remove_audio: bool,
        compress: bool,
    },
    ExtractAudio {
        format: AudioFormat,
    },
    Gif {
        width: Option<u32>,
    },
    TrimVideo {
        start: f64,
        end: Option<f64>,
        container: VideoContainer,
    },
    Audio {
        format: AudioFormat,
        normalize: bool,
        start: Option<f64>,
        end: Option<f64>,
    },
    Image {
        format: ImageFormat,
        width: Option<u32>,
        height: Option<u32>,
        compress: bool,
    },
}

#[derive(Debug, Clone)]
pub struct JobRequest {
    pub kind: JobKind,
    pub quality: Quality,
    pub prefer_hardware: bool,
    pub preserve_metadata: bool,
    pub encoder_override: Option<String>,
    pub bitrate_k: Option<u32>,
    pub output_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Ffmpeg,
    Convert,
}

#[derive(Debug, Clone)]
pub struct CommandPlan {
    pub program: String,
    pub tool: Tool,
    pub args: Vec<OsString>,
    pub output: PathBuf,
    pub expected_kind: MediaKind,
    pub expected_ext: String,
    pub remux: bool,
    pub used_hardware: bool,
    pub notes: Vec<String>,
    pub duration_hint: Option<f64>,
}

pub fn build_plan(probe: &Probe, request: &JobRequest) -> Result<CommandPlan> {
    let out_dir = resolve_output_dir(probe, request)?;
    paths::ensure_dir(&out_dir)?;
    match &request.kind {
        JobKind::Video { .. }
        | JobKind::ExtractAudio { .. }
        | JobKind::Gif { .. }
        | JobKind::TrimVideo { .. }
        | JobKind::Audio { .. } => build_ffmpeg_plan(probe, request, &out_dir),
        JobKind::Image { .. } => build_image_plan(probe, request, &out_dir),
    }
}

fn resolve_output_dir(probe: &Probe, request: &JobRequest) -> Result<PathBuf> {
    if let Some(dir) = &request.output_dir {
        return Ok(dir.clone());
    }
    probe
        .path
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| Error::user("Could not determine an output folder."))
}

fn stem_for(probe: &Probe, suffix: &str) -> String {
    let base = paths::file_stem(&probe.path);
    if suffix.is_empty() {
        base
    } else {
        format!("{base} {suffix}")
    }
}

fn build_ffmpeg_plan(probe: &Probe, request: &JobRequest, out_dir: &Path) -> Result<CommandPlan> {
    let caps = hw::detect();
    if !caps.ffmpeg_found {
        return Err(Error::user(
            "FFmpeg is not installed. Install the ffmpeg package to convert media.",
        ));
    }
    let ffmpeg = which::which("ffmpeg")
        .map_err(|_| Error::user("FFmpeg is not installed."))?
        .to_string_lossy()
        .into_owned();

    let mut args: Vec<OsString> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
    ];
    let mut notes = Vec::new();
    let mut remux = false;
    let mut used_hardware = false;

    match &request.kind {
        JobKind::Video {
            container,
            resolution,
            remove_audio,
            compress,
        } => {
            args.push("-i".into());
            args.push(probe.path.as_os_str().to_owned());
            let dest_ext = container.ext();
            let suffix = video_suffix(*resolution, *compress, *remove_audio);
            let output = paths::unique_output_path(
                &probe.path,
                out_dir,
                &stem_for(probe, &suffix),
                dest_ext,
            );

            let can_copy_video = can_stream_copy_video(probe, *container, *resolution, *compress);
            let can_copy_audio = !*remove_audio && can_stream_copy_audio(probe, *container);

            if *remove_audio {
                args.push("-an".into());
            }

            if can_copy_video && (*remove_audio || can_copy_audio) && request.encoder_override.is_none()
            {
                remux = true;
                args.push("-c:v".into());
                args.push("copy".into());
                if !*remove_audio {
                    args.push("-c:a".into());
                    args.push("copy".into());
                }
                notes.push("Copied the existing streams without re-encoding.".into());
            } else {
                apply_scale(&mut args, probe, *resolution, &mut notes);
                let (enc, hw) = pick_video_encoder(
                    *container,
                    request,
                    &caps,
                    probe.duration_secs,
                )?;
                used_hardware = hw;
                apply_video_encoder(&mut args, &enc, request, &caps, &mut notes)?;
                if !*remove_audio {
                    if probe.has_audio {
                        apply_audio_encoder(&mut args, *container, request, &caps)?;
                    } else {
                        args.push("-an".into());
                    }
                }
            }

            apply_metadata_flag(&mut args, request.preserve_metadata);
            apply_container_flags(&mut args, *container);
            args.push(output.as_os_str().to_owned());

            return Ok(CommandPlan {
                program: ffmpeg,
                tool: Tool::Ffmpeg,
                args,
                output,
                expected_kind: MediaKind::Video,
                expected_ext: dest_ext.into(),
                remux,
                used_hardware,
                notes,
                duration_hint: probe.duration_secs,
            });
        }
        JobKind::ExtractAudio { format } => {
            if !probe.has_audio {
                return Err(Error::user(
                    "This video has no audio track to extract.",
                ));
            }
            args.push("-i".into());
            args.push(probe.path.as_os_str().to_owned());
            args.push("-vn".into());
            apply_audio_format(&mut args, *format, request, &caps)?;
            apply_metadata_flag(&mut args, request.preserve_metadata);
            let output = paths::unique_output_path(
                &probe.path,
                out_dir,
                &stem_for(probe, ""),
                format.ext(),
            );
            args.push(output.as_os_str().to_owned());
            return Ok(CommandPlan {
                program: ffmpeg,
                tool: Tool::Ffmpeg,
                args,
                output,
                expected_kind: MediaKind::Audio,
                expected_ext: format.ext().into(),
                remux: false,
                used_hardware: false,
                notes,
                duration_hint: probe.duration_secs,
            });
        }
        JobKind::Gif { width } => {
            args.push("-i".into());
            args.push(probe.path.as_os_str().to_owned());
            let w = width.unwrap_or(480);
            let fps = request.quality.gif_fps();
            let vf = format!(
                "fps={fps},scale={w}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer"
            );
            args.push("-vf".into());
            args.push(vf.into());
            args.push("-loop".into());
            args.push("0".into());
            args.push("-an".into());
            let output =
                paths::unique_output_path(&probe.path, out_dir, &stem_for(probe, ""), "gif");
            args.push(output.as_os_str().to_owned());
            notes.push(format!("Animated GIF at {fps} fps, width {w}px."));
            return Ok(CommandPlan {
                program: ffmpeg,
                tool: Tool::Ffmpeg,
                args,
                output,
                expected_kind: MediaKind::Image,
                expected_ext: "gif".into(),
                remux: false,
                used_hardware: false,
                notes,
                duration_hint: probe.duration_secs,
            });
        }
        JobKind::TrimVideo {
            start,
            end,
            container,
        } => {
            if *start < 0.0 {
                return Err(Error::user("Trim start cannot be negative."));
            }
            if let Some(end) = end {
                if *end <= *start {
                    return Err(Error::user("Trim end must be after the start time."));
                }
            }
            // Accurate trim: input first, then -ss/-to (re-encode).
            args.push("-i".into());
            args.push(probe.path.as_os_str().to_owned());
            args.push("-ss".into());
            args.push(format_ts(*start).into());
            if let Some(end) = end {
                args.push("-to".into());
                args.push(format_ts(*end).into());
            }
            apply_scale(&mut args, probe, ResolutionTarget::Original, &mut notes);
            let (enc, hw) = pick_video_encoder(*container, request, &caps, None)?;
            used_hardware = hw;
            apply_video_encoder(&mut args, &enc, request, &caps, &mut notes)?;
            if probe.has_audio {
                apply_audio_encoder(&mut args, *container, request, &caps)?;
            } else {
                args.push("-an".into());
            }
            apply_metadata_flag(&mut args, request.preserve_metadata);
            apply_container_flags(&mut args, *container);
            let output = paths::unique_output_path(
                &probe.path,
                out_dir,
                &stem_for(probe, "(trimmed)"),
                container.ext(),
            );
            args.push(output.as_os_str().to_owned());
            let hint = match end {
                Some(e) => Some((e - start).max(0.0)),
                None => probe.duration_secs.map(|d| (d - start).max(0.0)),
            };
            return Ok(CommandPlan {
                program: ffmpeg,
                tool: Tool::Ffmpeg,
                args,
                output,
                expected_kind: MediaKind::Video,
                expected_ext: container.ext().into(),
                remux: false,
                used_hardware,
                notes,
                duration_hint: hint,
            });
        }
        JobKind::Audio {
            format,
            normalize,
            start,
            end,
        } => {
            if !probe.has_audio && probe.kind != MediaKind::Audio {
                return Err(Error::user("This file has no audio to convert."));
            }
            args.push("-i".into());
            args.push(probe.path.as_os_str().to_owned());
            args.push("-vn".into());
            if let Some(s) = start {
                args.push("-ss".into());
                args.push(format_ts(*s).into());
            }
            if let Some(e) = end {
                args.push("-to".into());
                args.push(format_ts(*e).into());
            }
            if *normalize {
                args.push("-af".into());
                args.push("loudnorm=I=-16:TP=-1.5:LRA=11".into());
                notes.push("Loudness normalized.".into());
            }
            apply_audio_format(&mut args, *format, request, &caps)?;
            apply_metadata_flag(&mut args, request.preserve_metadata);
            let suffix = if *normalize { "(normalized)" } else { "" };
            let output = paths::unique_output_path(
                &probe.path,
                out_dir,
                &stem_for(probe, suffix),
                format.ext(),
            );
            args.push(output.as_os_str().to_owned());
            return Ok(CommandPlan {
                program: ffmpeg,
                tool: Tool::Ffmpeg,
                args,
                output,
                expected_kind: MediaKind::Audio,
                expected_ext: format.ext().into(),
                remux: false,
                used_hardware: false,
                notes,
                duration_hint: probe.duration_secs,
            });
        }
        JobKind::Image { .. } => unreachable!(),
    }
}

fn build_image_plan(probe: &Probe, request: &JobRequest, out_dir: &Path) -> Result<CommandPlan> {
    let JobKind::Image {
        format,
        width,
        height,
        compress,
    } = &request.kind
    else {
        unreachable!();
    };
    let output = paths::unique_output_path(
        &probe.path,
        out_dir,
        &stem_for(probe, if *compress { "(compressed)" } else { "" }),
        format.ext(),
    );

    let quality = if *compress {
        request.quality.image_quality().saturating_sub(12).max(40)
    } else {
        request.quality.image_quality()
    };

    // Prefer ImageMagick for images: EXIF orientation, profiles, resize.
    let use_convert = which::which("convert").is_ok()
        && (*format != ImageFormat::Avif || convert_supports_avif());
    if use_convert {
        return build_convert_image(probe, *format, *width, *height, quality, request, output);
    }

    build_ffmpeg_image(probe, *format, *width, *height, quality, request, output)
}

fn convert_supports_avif() -> bool {
    which::which("convert").is_ok() && {
        let out = std::process::Command::new("convert")
            .arg("-list")
            .arg("format")
            .output();
        out.ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("AVIF"))
            .unwrap_or(false)
    }
}

fn build_convert_image(
    probe: &Probe,
    format: ImageFormat,
    width: Option<u32>,
    height: Option<u32>,
    quality: u8,
    request: &JobRequest,
    output: PathBuf,
) -> Result<CommandPlan> {
    let convert = which::which("convert")
        .map_err(|_| Error::user("ImageMagick convert is not available."))?
        .to_string_lossy()
        .into_owned();
    let mut args: Vec<OsString> = Vec::new();
    args.push(probe.path.as_os_str().to_owned());
    args.push("-auto-orient".into());
    if let Some(w) = width {
        let h = height.unwrap_or(w);
        // Only shrink; never silent-upscale.
        args.push("-resize".into());
        args.push(format!("{w}x{h}>").into());
    }
    if !request.preserve_metadata {
        args.push("-strip".into());
    }
    match format {
        ImageFormat::Jpeg | ImageFormat::Webp => {
            args.push("-quality".into());
            args.push(quality.to_string().into());
        }
        ImageFormat::Png => {
            args.push("-quality".into());
            args.push(png_quality(quality).into());
        }
        ImageFormat::Avif => {
            args.push("-quality".into());
            args.push(quality.to_string().into());
        }
    }
    args.push(output.as_os_str().to_owned());
    let mut notes = vec!["Respects EXIF orientation.".into()];
    if width.is_some() {
        notes.push("Resized without upscaling.".into());
    }
    Ok(CommandPlan {
        program: convert,
        tool: Tool::Convert,
        args,
        output,
        expected_kind: MediaKind::Image,
        expected_ext: format.ext().into(),
        remux: false,
        used_hardware: false,
        notes,
        duration_hint: None,
    })
}

fn build_ffmpeg_image(
    probe: &Probe,
    format: ImageFormat,
    width: Option<u32>,
    height: Option<u32>,
    quality: u8,
    request: &JobRequest,
    output: PathBuf,
) -> Result<CommandPlan> {
    let caps = hw::detect();
    let ffmpeg = which::which("ffmpeg")
        .map_err(|_| Error::user("FFmpeg is not installed."))?
        .to_string_lossy()
        .into_owned();
    let mut args: Vec<OsString> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
        "-i".into(),
        probe.path.as_os_str().to_owned(),
        "-frames:v".into(),
        "1".into(),
    ];
    if let Some(w) = width {
        let h = height.unwrap_or(-1i32 as u32);
        let scale = if h == u32::MAX {
            format!("scale={w}:-1:flags=lanczos")
        } else {
            format!("scale='min({w},iw)':-1:flags=lanczos")
        };
        let _ = h;
        args.push("-vf".into());
        args.push(format!("scale='min({w},iw)':-1:flags=lanczos").into());
        let _ = scale;
    }
    match format {
        ImageFormat::Png => {
            args.push("-c:v".into());
            args.push("png".into());
        }
        ImageFormat::Jpeg => {
            args.push("-c:v".into());
            args.push("mjpeg".into());
            args.push("-q:v".into());
            args.push(jpeg_q(quality).into());
        }
        ImageFormat::Webp => {
            if !caps.libwebp {
                return Err(Error::user(
                    "This FFmpeg build cannot write WebP. Install a build with libwebp, or ImageMagick.",
                ));
            }
            args.push("-c:v".into());
            args.push("libwebp".into());
            args.push("-quality".into());
            args.push(quality.to_string().into());
        }
        ImageFormat::Avif => {
            if !caps.libaom_still {
                return Err(Error::user(
                    "This FFmpeg build cannot write AVIF (needs libaom-av1).",
                ));
            }
            args.push("-c:v".into());
            args.push("libaom-av1".into());
            args.push("-still-picture".into());
            args.push("1".into());
            args.push("-crf".into());
            args.push((63 - (quality as u32 * 50 / 100)).clamp(18, 50).to_string().into());
        }
    }
    apply_metadata_flag(&mut args, request.preserve_metadata);
    args.push(output.as_os_str().to_owned());
    Ok(CommandPlan {
        program: ffmpeg,
        tool: Tool::Ffmpeg,
        args,
        output,
        expected_kind: MediaKind::Image,
        expected_ext: format.ext().into(),
        remux: false,
        used_hardware: false,
        notes: vec!["Image converted with FFmpeg.".into()],
        duration_hint: None,
    })
}

fn png_quality(q: u8) -> String {
    // ImageMagick PNG quality: 0-100, higher is smaller compression effort mix.
    format!("{}", 100 - (q as u32 / 2))
}

fn jpeg_q(quality: u8) -> String {
    // ffmpeg mjpeg q:v 2-31, lower is better
    let q = 2 + ((100 - quality as u32) * 20 / 100);
    q.clamp(2, 31).to_string()
}

fn video_suffix(res: ResolutionTarget, compress: bool, remove_audio: bool) -> String {
    let mut parts = Vec::new();
    if res != ResolutionTarget::Original {
        parts.push(res.label().to_string());
    }
    if compress {
        parts.push("compressed".into());
    }
    if remove_audio {
        parts.push("no audio".into());
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("({})", parts.join(", "))
    }
}

fn apply_scale(
    args: &mut Vec<OsString>,
    probe: &Probe,
    target: ResolutionTarget,
    notes: &mut Vec<String>,
) {
    let Some(want_h) = target.height() else {
        return;
    };
    if let Some(src_h) = probe.height {
        if src_h <= want_h {
            notes.push(format!(
                "Source is {src_h}p, so it was not upscaled to {}.",
                target.label()
            ));
            return;
        }
    }
    args.push("-vf".into());
    args.push(format!("scale=-2:{want_h}:flags=lanczos").into());
}

fn can_stream_copy_video(
    probe: &Probe,
    container: VideoContainer,
    resolution: ResolutionTarget,
    compress: bool,
) -> bool {
    if compress || resolution != ResolutionTarget::Original {
        return false;
    }
    let Some(codec) = probe.video_codec.as_deref() else {
        return false;
    };
    match container {
        VideoContainer::Mp4 | VideoContainer::Mkv => {
            matches!(codec, "h264" | "hevc" | "av1" | "mpeg4")
        }
        VideoContainer::Webm => matches!(codec, "vp8" | "vp9" | "av1"),
    }
}

fn can_stream_copy_audio(probe: &Probe, container: VideoContainer) -> bool {
    let Some(codec) = probe.audio_codec.as_deref() else {
        return true;
    };
    match container {
        VideoContainer::Mp4 => matches!(codec, "aac" | "mp3" | "alac" | "ac3"),
        VideoContainer::Mkv => true,
        VideoContainer::Webm => matches!(codec, "opus" | "vorbis"),
    }
}

fn pick_video_encoder(
    container: VideoContainer,
    request: &JobRequest,
    caps: &HardwareCaps,
    duration: Option<f64>,
) -> Result<(String, bool)> {
    if let Some(over) = &request.encoder_override {
        if over != "auto" {
            return Ok((over.clone(), over.contains("nvenc") || over.contains("vaapi")));
        }
    }
    match container {
        VideoContainer::Webm => {
            if caps.software_vp9 {
                Ok(("libvpx-vp9".into(), false))
            } else if caps.software_aom_av1 {
                Ok(("libaom-av1".into(), false))
            } else {
                Err(Error::user(
                    "This FFmpeg build cannot write WebM (needs VP9 or AV1).",
                ))
            }
        }
        VideoContainer::Mp4 | VideoContainer::Mkv => {
            let use_nvenc = request.prefer_hardware
                && caps.nvenc_h264
                && hw::nvenc_worthwhile(duration, true);
            if use_nvenc {
                Ok(("h264_nvenc".into(), true))
            } else if caps.software_x264 {
                Ok(("libx264".into(), false))
            } else if caps.nvenc_h264 {
                Ok(("h264_nvenc".into(), true))
            } else {
                Err(Error::user(
                    "No H.264 encoder is available. Install FFmpeg with libx264.",
                ))
            }
        }
    }
}

fn apply_video_encoder(
    args: &mut Vec<OsString>,
    encoder: &str,
    request: &JobRequest,
    _caps: &HardwareCaps,
    notes: &mut Vec<String>,
) -> Result<()> {
    args.push("-c:v".into());
    args.push(encoder.into());
    if let Some(br) = request.bitrate_k {
        args.push("-b:v".into());
        args.push(format!("{br}k").into());
        notes.push(format!("Video bitrate {br} kbps."));
        return Ok(());
    }
    match encoder {
        "h264_nvenc" | "hevc_nvenc" | "av1_nvenc" => {
            args.push("-preset".into());
            args.push("p4".into());
            args.push("-rc".into());
            args.push("vbr".into());
            args.push("-cq".into());
            args.push(request.quality.nvenc_cq().to_string().into());
            notes.push(format!("Hardware encoder {encoder}."));
        }
        "libx264" => {
            args.push("-preset".into());
            args.push("medium".into());
            args.push("-crf".into());
            args.push(request.quality.video_crf().to_string().into());
            args.push("-pix_fmt".into());
            args.push("yuv420p".into());
        }
        "libx265" => {
            args.push("-crf".into());
            args.push(request.quality.video_crf().to_string().into());
        }
        "libvpx-vp9" => {
            args.push("-b:v".into());
            args.push("0".into());
            args.push("-crf".into());
            args.push(request.quality.video_crf().to_string().into());
            args.push("-row-mt".into());
            args.push("1".into());
        }
        "libaom-av1" => {
            args.push("-crf".into());
            args.push(request.quality.video_crf().to_string().into());
            args.push("-cpu-used".into());
            args.push("6".into());
        }
        other => {
            notes.push(format!("Encoder {other}."));
        }
    }
    Ok(())
}

fn apply_audio_encoder(
    args: &mut Vec<OsString>,
    container: VideoContainer,
    request: &JobRequest,
    caps: &HardwareCaps,
) -> Result<()> {
    match container {
        VideoContainer::Webm => apply_audio_format(args, AudioFormat::Opus, request, caps),
        VideoContainer::Mp4 | VideoContainer::Mkv => {
            apply_audio_format(args, AudioFormat::Aac, request, caps)
        }
    }
}

fn apply_audio_format(
    args: &mut Vec<OsString>,
    format: AudioFormat,
    request: &JobRequest,
    caps: &HardwareCaps,
) -> Result<()> {
    match format {
        AudioFormat::Mp3 => {
            if !caps.libmp3lame {
                return Err(Error::user(
                    "This FFmpeg build cannot write MP3 (needs libmp3lame).",
                ));
            }
            args.push("-c:a".into());
            args.push("libmp3lame".into());
            args.push("-b:a".into());
            args.push(format!("{}k", request.quality.audio_bitrate_k()).into());
        }
        AudioFormat::Wav => {
            args.push("-c:a".into());
            args.push("pcm_s16le".into());
        }
        AudioFormat::Flac => {
            if !caps.flac {
                return Err(Error::user("This FFmpeg build cannot write FLAC."));
            }
            args.push("-c:a".into());
            args.push("flac".into());
        }
        AudioFormat::Aac => {
            if !caps.aac {
                return Err(Error::user("This FFmpeg build cannot write AAC."));
            }
            args.push("-c:a".into());
            args.push("aac".into());
            args.push("-b:a".into());
            args.push(format!("{}k", request.quality.audio_bitrate_k()).into());
        }
        AudioFormat::Opus => {
            if !caps.libopus {
                return Err(Error::user("This FFmpeg build cannot write Opus."));
            }
            args.push("-c:a".into());
            args.push("libopus".into());
            args.push("-b:a".into());
            args.push(format!("{}k", request.quality.audio_bitrate_k().min(192)).into());
        }
    }
    Ok(())
}

fn apply_metadata_flag(args: &mut Vec<OsString>, preserve: bool) {
    if preserve {
        args.push("-map_metadata".into());
        args.push("0".into());
    } else {
        args.push("-map_metadata".into());
        args.push("-1".into());
    }
}

fn apply_container_flags(args: &mut Vec<OsString>, container: VideoContainer) {
    if container == VideoContainer::Mp4 {
        args.push("-movflags".into());
        args.push("+faststart".into());
    }
}

fn format_ts(secs: f64) -> String {
    if secs < 0.0 {
        return "0".into();
    }
    format!("{secs:.3}")
}

pub fn estimate_size_bytes(probe: &Probe, request: &JobRequest) -> Option<u64> {
    match &request.kind {
        JobKind::Image { .. } => {
            let pixels = probe.width.unwrap_or(1280) as u64 * probe.height.unwrap_or(720) as u64;
            let q = request.quality.image_quality() as u64;
            Some(pixels * q / 80)
        }
        JobKind::ExtractAudio { format } | JobKind::Audio { format, .. } => {
            let dur = probe.duration_secs?;
            let bps = match format {
                AudioFormat::Wav => 44100 * 2 * 2,
                AudioFormat::Flac => 700_000 / 8,
                _ => request.quality.audio_bitrate_k() as u64 * 1000 / 8,
            };
            Some((dur * bps as f64) as u64)
        }
        JobKind::Gif { width } => {
            let dur = probe.duration_secs.unwrap_or(3.0);
            let w = width.unwrap_or(480) as f64;
            Some((dur * w * 80.0) as u64)
        }
        JobKind::Video { .. } | JobKind::TrimVideo { .. } => {
            let dur = match &request.kind {
                JobKind::TrimVideo { start, end, .. } => match end {
                    Some(e) => (*e - *start).max(0.0),
                    None => probe.duration_secs.map(|d| (d - *start).max(0.0))?,
                },
                _ => probe.duration_secs?,
            };
            let compressed = matches!(&request.kind, JobKind::Video { compress: true, .. });
            let factor = if compressed {
                0.45
            } else {
                match request.quality {
                    Quality::Small => 0.35,
                    Quality::Balanced => 0.55,
                    Quality::High => 0.8,
                    Quality::Maximum => 1.15,
                }
            };
            Some(
                (probe.size_bytes as f64 * factor * (dur / probe.duration_secs.unwrap_or(dur)))
                    as u64,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_copy_h264_to_mp4() {
        let probe = Probe {
            path: PathBuf::from("clip.mkv"),
            kind: MediaKind::Video,
            format_name: Some("matroska".into()),
            duration_secs: Some(10.0),
            size_bytes: 1000,
            bit_rate: None,
            width: Some(1280),
            height: Some(720),
            fps: Some(30.0),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            sample_rate: Some(48000),
            channels: Some(2),
            has_video: true,
            has_audio: true,
            rotation: None,
            raw_json: String::new(),
        };
        assert!(can_stream_copy_video(
            &probe,
            VideoContainer::Mp4,
            ResolutionTarget::Original,
            false
        ));
        assert!(!can_stream_copy_video(
            &probe,
            VideoContainer::Mp4,
            ResolutionTarget::P1080,
            false
        ));
    }
}

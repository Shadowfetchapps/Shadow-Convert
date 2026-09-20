use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use shadow_convert::plan::{
    AudioFormat, ImageFormat, JobKind, JobRequest, ResolutionTarget, VideoContainer,
};
use shadow_convert::settings::Quality;
use shadow_convert::{convert, probe_file};

fn ffmpeg() -> Command {
    Command::new("ffmpeg")
}

fn make_video(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let status = ffmpeg()
        .args([
            "-hide_banner",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=320x240:rate=15",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-shortest",
        ])
        .arg(&path)
        .status()
        .expect("ffmpeg video");
    assert!(status.success(), "failed to create test video");
    path
}

fn make_audio(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let status = ffmpeg()
        .args([
            "-hide_banner",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=330:duration=1.5",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "96k",
        ])
        .arg(&path)
        .status()
        .expect("ffmpeg audio");
    assert!(status.success(), "failed to create test audio");
    path
}

fn make_png(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let status = ffmpeg()
        .args([
            "-hide_banner",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=96x64",
            "-frames:v",
            "1",
            "-update",
            "1",
        ])
        .arg(&path)
        .status()
        .expect("ffmpeg png");
    assert!(status.success(), "failed to create test image");
    path
}

fn request(kind: JobKind, quality: Quality) -> JobRequest {
    JobRequest {
        kind,
        quality,
        prefer_hardware: false,
        preserve_metadata: true,
        encoder_override: Some("libx264".into()),
        bitrate_k: None,
        output_dir: None,
    }
}

#[test]
fn probe_real_video() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_video(dir.path(), "clip.mp4");
    let probe = probe_file(&path).unwrap();
    assert!(probe.has_video);
    assert!(probe.has_audio);
    assert!(probe.width.unwrap() >= 320);
    assert!(probe.duration_secs.unwrap() > 1.0);
}

#[test]
fn convert_video_to_mp4_validates() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_video(dir.path(), "source.mkv");
    let probe = probe_file(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let result = convert(
        &probe,
        &request(
            JobKind::Video {
                container: VideoContainer::Mp4,
                resolution: ResolutionTarget::Original,
                remove_audio: false,
                compress: false,
            },
            Quality::High,
        ),
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .expect("convert video");
    assert!(result.output.exists());
    assert_ne!(result.output, path);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let out = probe_file(&result.output).unwrap();
    assert!(out.has_video);
    assert!(out.has_audio);
}

#[test]
fn extract_audio_mp3() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_video(dir.path(), "talk.mp4");
    let probe = probe_file(&path).unwrap();
    let result = convert(
        &probe,
        &JobRequest {
            kind: JobKind::ExtractAudio {
                format: AudioFormat::Mp3,
            },
            quality: Quality::High,
            prefer_hardware: false,
            preserve_metadata: true,
            encoder_override: None,
            bitrate_k: None,
            output_dir: None,
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .expect("extract audio");
    let out = probe_file(&result.output).unwrap();
    assert!(out.has_audio);
    assert!(!out.has_video);
    assert_eq!(result.output.extension().unwrap(), "mp3");
}

#[test]
fn quality_affects_audio_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_audio(dir.path(), "tone.mp3");
    let probe = probe_file(&path).unwrap();
    let small = convert(
        &probe,
        &JobRequest {
            kind: JobKind::Audio {
                format: AudioFormat::Mp3,
                normalize: false,
                start: None,
                end: None,
            },
            quality: Quality::Small,
            prefer_hardware: false,
            preserve_metadata: true,
            encoder_override: None,
            bitrate_k: None,
            output_dir: None,
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .unwrap();
    let high = convert(
        &probe,
        &JobRequest {
            kind: JobKind::Audio {
                format: AudioFormat::Mp3,
                normalize: false,
                start: None,
                end: None,
            },
            quality: Quality::Maximum,
            prefer_hardware: false,
            preserve_metadata: true,
            encoder_override: None,
            bitrate_k: None,
            output_dir: None,
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .unwrap();
    let small_len = std::fs::metadata(&small.output).unwrap().len();
    let high_len = std::fs::metadata(&high.output).unwrap().len();
    assert!(
        high_len > small_len,
        "maximum ({high_len}) should be larger than small ({small_len})"
    );
}

#[test]
fn convert_image_jpeg() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_png(dir.path(), "pic.png");
    let probe = probe_file(&path).unwrap();
    let result = convert(
        &probe,
        &JobRequest {
            kind: JobKind::Image {
                format: ImageFormat::Jpeg,
                width: None,
                height: None,
                compress: false,
            },
            quality: Quality::High,
            prefer_hardware: false,
            preserve_metadata: true,
            encoder_override: None,
            bitrate_k: None,
            output_dir: None,
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .expect("image convert");
    let out = probe_file(&result.output).unwrap();
    assert!(out.width.unwrap() >= 90);
    assert!(result.output.extension().unwrap() == "jpg");
}

#[test]
fn unicode_filename_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_png(dir.path(), "holiday 🏖️ photo.png");
    let probe = probe_file(&path).unwrap();
    let result = convert(
        &probe,
        &JobRequest {
            kind: JobKind::Image {
                format: ImageFormat::Png,
                width: Some(64),
                height: Some(64),
                compress: false,
            },
            quality: Quality::High,
            prefer_hardware: false,
            preserve_metadata: true,
            encoder_override: None,
            bitrate_k: None,
            output_dir: None,
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .expect("unicode name");
    assert!(result.output.exists());
    assert!(path.exists());
}

#[test]
fn never_overwrite_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_png(dir.path(), "same.png");
    let probe = probe_file(&path).unwrap();
    let result = convert(
        &probe,
        &JobRequest {
            kind: JobKind::Image {
                format: ImageFormat::Png,
                width: None,
                height: None,
                compress: false,
            },
            quality: Quality::High,
            prefer_hardware: false,
            preserve_metadata: true,
            encoder_override: None,
            bitrate_k: None,
            output_dir: None,
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .unwrap();
    assert_ne!(
        result.output.canonicalize().unwrap(),
        path.canonicalize().unwrap()
    );
}

#[test]
fn hardware_detect_has_cpu_fallback() {
    let caps = shadow_convert::hw::detect_now();
    assert!(caps.ffmpeg_found);
    assert!(
        caps.software_x264 || caps.nvenc_h264,
        "need a usable H.264 path"
    );
}

#[test]
fn cancel_stops_long_encode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.mp4");
    let status = ffmpeg()
        .args([
            "-hide_banner",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=30:size=640x360:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=220:duration=30",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    let probe = probe_file(&path).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_t = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        cancel_t.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let result = convert(
        &probe,
        &JobRequest {
            kind: JobKind::Video {
                container: VideoContainer::Mp4,
                resolution: ResolutionTarget::P1080,
                remove_audio: false,
                compress: true,
            },
            quality: Quality::Maximum,
            prefer_hardware: false,
            preserve_metadata: true,
            encoder_override: Some("libx264".into()),
            bitrate_k: None,
            output_dir: None,
        },
        cancel,
        |_| {},
    );
    assert!(result.is_err(), "cancel should fail the job");
}

#[test]
fn remove_audio_leaves_video() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_video(dir.path(), "with-sound.mp4");
    let probe = probe_file(&path).unwrap();
    let result = convert(
        &probe,
        &request(
            JobKind::Video {
                container: VideoContainer::Mp4,
                resolution: ResolutionTarget::Original,
                remove_audio: true,
                compress: false,
            },
            Quality::High,
        ),
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .unwrap();
    let out = probe_file(&result.output).unwrap();
    assert!(out.has_video);
    assert!(!out.has_audio);
}

#[test]
fn empty_file_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.mp4");
    std::fs::write(&path, b"").unwrap();
    assert!(probe_file(&path).is_err());
}

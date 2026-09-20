use std::process::Command;
use std::sync::OnceLock;

/// Detected encoder capabilities. Never assumes a particular GPU.
#[derive(Debug, Clone, Default)]
pub struct HardwareCaps {
    pub ffmpeg_found: bool,
    pub nvenc_h264: bool,
    pub nvenc_hevc: bool,
    pub nvenc_av1: bool,
    pub vaapi_h264: bool,
    pub vaapi_hevc: bool,
    pub software_x264: bool,
    pub software_x265: bool,
    pub software_vp9: bool,
    pub software_aom_av1: bool,
    pub software_svtav1: bool,
    pub aac: bool,
    pub libmp3lame: bool,
    pub flac: bool,
    pub libopus: bool,
    pub libwebp: bool,
    pub libaom_still: bool,
    pub raw_encoders: String,
}

impl HardwareCaps {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.nvenc_h264 {
            parts.push("NVENC H.264");
        }
        if self.nvenc_hevc {
            parts.push("NVENC HEVC");
        }
        if self.nvenc_av1 {
            parts.push("NVENC AV1");
        }
        if self.vaapi_h264 {
            parts.push("VAAPI H.264");
        }
        if parts.is_empty() {
            "Software (CPU)".into()
        } else {
            parts.join(", ")
        }
    }

    pub fn has_nvenc(&self) -> bool {
        self.nvenc_h264 || self.nvenc_hevc || self.nvenc_av1
    }

    pub fn video_encoder_choices(&self) -> Vec<(&'static str, &'static str)> {
        let mut v = vec![("auto", "Auto")];
        if self.nvenc_h264 {
            v.push(("h264_nvenc", "NVENC H.264"));
        }
        if self.nvenc_hevc {
            v.push(("hevc_nvenc", "NVENC HEVC"));
        }
        if self.nvenc_av1 {
            v.push(("av1_nvenc", "NVENC AV1"));
        }
        if self.software_x264 {
            v.push(("libx264", "CPU H.264"));
        }
        if self.software_vp9 {
            v.push(("libvpx-vp9", "CPU VP9"));
        }
        if self.software_aom_av1 {
            v.push(("libaom-av1", "CPU AV1"));
        }
        v
    }
}

pub fn detect() -> HardwareCaps {
    static CACHED: OnceLock<HardwareCaps> = OnceLock::new();
    CACHED.get_or_init(detect_now).clone()
}

pub fn detect_now() -> HardwareCaps {
    let mut caps = HardwareCaps::default();
    let Ok(ffmpeg) = which::which("ffmpeg") else {
        return caps;
    };
    caps.ffmpeg_found = true;
    let output = Command::new(&ffmpeg)
        .arg("-hide_banner")
        .arg("-encoders")
        .output();
    let Ok(output) = output else {
        return caps;
    };
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    caps.raw_encoders = text.clone();
    let has = |name: &str| {
        text.lines().any(|line| {
            line.split_whitespace()
                .nth(1)
                .is_some_and(|token| token == name)
        })
    };
    caps.nvenc_h264 = has("h264_nvenc");
    caps.nvenc_hevc = has("hevc_nvenc");
    caps.nvenc_av1 = has("av1_nvenc");
    caps.vaapi_h264 = has("h264_vaapi");
    caps.vaapi_hevc = has("hevc_vaapi");
    caps.software_x264 = has("libx264");
    caps.software_x265 = has("libx265");
    caps.software_vp9 = has("libvpx-vp9");
    caps.software_aom_av1 = has("libaom-av1");
    caps.software_svtav1 = has("libsvtav1");
    caps.aac = has("aac");
    caps.libmp3lame = has("libmp3lame");
    caps.flac = has("flac");
    caps.libopus = has("libopus");
    caps.libwebp = has("libwebp");
    caps.libaom_still = caps.software_aom_av1;
    caps
}

/// NVENC is helpful for longer video re-encodes. Skip it for remux, images,
/// and very short clips where process startup dominates.
pub fn nvenc_worthwhile(duration_secs: Option<f64>, reencode: bool) -> bool {
    reencode && duration_secs.unwrap_or(0.0) >= 4.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_does_not_panic() {
        let caps = detect_now();
        assert!(caps.ffmpeg_found);
        assert!(caps.software_x264 || caps.nvenc_h264);
    }

    #[test]
    fn short_clips_skip_nvenc() {
        assert!(!nvenc_worthwhile(Some(1.0), true));
        assert!(nvenc_worthwhile(Some(30.0), true));
        assert!(!nvenc_worthwhile(Some(30.0), false));
    }
}

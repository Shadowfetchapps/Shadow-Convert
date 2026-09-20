use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Audio,
    Image,
    Unknown,
}

impl MediaKind {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Image => "Image",
            Self::Unknown => "Unknown",
        }
    }
}

/// Inspect file bytes. Extension is only a hint after content.
pub fn sniff_kind(path: &Path) -> MediaKind {
    let mut buf = [0u8; 64];
    let n = File::open(path)
        .and_then(|mut f| f.read(&mut buf))
        .unwrap_or(0);
    if n == 0 {
        return MediaKind::Unknown;
    }
    let b = &buf[..n];
    if looks_image(b) {
        return MediaKind::Image;
    }
    if looks_audio(b) {
        return MediaKind::Audio;
    }
    if looks_video_or_container(b) {
        return MediaKind::Video;
    }
    MediaKind::Unknown
}

fn looks_image(b: &[u8]) -> bool {
    b.starts_with(&[0x89, b'P', b'N', b'G'])
        || b.starts_with(&[0xFF, 0xD8, 0xFF])
        || (b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP")
        || b.starts_with(b"GIF87a")
        || b.starts_with(b"GIF89a")
        || (b.len() >= 12 && &b[4..8] == b"ftyp" && looks_avif_ftyp(b))
        || b.starts_with(&[0x49, 0x49, 0x2A, 0x00])
        || b.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
        || b.starts_with(b"BM")
}

fn looks_avif_ftyp(b: &[u8]) -> bool {
    let hay = &b[..b.len().min(32)];
    hay.windows(4).any(|w| w == b"avif" || w == b"avis")
}

fn looks_audio(b: &[u8]) -> bool {
    b.starts_with(b"ID3")
        || (b.len() >= 2 && b[0] == 0xFF && (b[1] & 0xE0) == 0xE0)
        || b.starts_with(b"fLaC")
        || b.starts_with(b"OggS")
        || (b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WAVE")
        || b.starts_with(&[0xFF, 0xF1])
        || b.starts_with(&[0xFF, 0xF9])
}

fn looks_video_or_container(b: &[u8]) -> bool {
    (b.len() >= 12 && &b[4..8] == b"ftyp")
        || b.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])
        || (b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"AVI ")
        || b.starts_with(&[0x00, 0x00, 0x00, 0x14]) // some MP4
        || b.windows(4).any(|w| w == b"moov" || w == b"mdat")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_magic() {
        let mut hdr = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        hdr.extend_from_slice(&[0; 16]);
        assert!(looks_image(&hdr));
    }

    #[test]
    fn jpeg_magic() {
        assert!(looks_image(&[0xFF, 0xD8, 0xFF, 0xE0]));
    }
}

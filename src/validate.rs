use std::path::Path;

use crate::detect::MediaKind;
use crate::error::{Error, Result};
use crate::plan::CommandPlan;
use crate::probe;

pub fn validate_output(path: &Path, plan: &CommandPlan) -> Result<()> {
    if !path.is_file() {
        return Err(Error::user("The output file was not created."));
    }
    let meta = std::fs::metadata(path)?;
    if meta.len() == 0 {
        return Err(Error::user("The output file is empty."));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ext.is_empty() && ext != plan.expected_ext && !(plan.expected_ext == "jpg" && ext == "jpeg")
    {
        return Err(Error::detailed(
            "The output file has the wrong extension.",
            format!("expected {} got {ext}", plan.expected_ext),
        ));
    }

    let probed = probe::probe(path).map_err(|err| {
        Error::detailed(
            "The output file could not be read back. The conversion is considered failed.",
            err.human_message(),
        )
    })?;

    if probed.kind != plan.expected_kind && !(plan.expected_ext == "gif") {
        return Err(Error::detailed(
            "The output file is not the expected media type.",
            format!(
                "expected {:?} got {:?}",
                plan.expected_kind, probed.kind
            ),
        ));
    }

    match plan.expected_kind {
        MediaKind::Video => {
            if !probed.has_video {
                return Err(Error::user("The output video has no video track."));
            }
            if probed.width.unwrap_or(0) < 2 || probed.height.unwrap_or(0) < 2 {
                return Err(Error::user("The output video has an invalid size."));
            }
        }
        MediaKind::Audio => {
            if !probed.has_audio {
                return Err(Error::user("The output audio file has no audio track."));
            }
        }
        MediaKind::Image => {
            if probed.width.unwrap_or(0) < 1 || probed.height.unwrap_or(0) < 1 {
                return Err(Error::user("The output image has an invalid size."));
            }
        }
        MediaKind::Unknown => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{CommandPlan, Tool};
    use std::ffi::OsString;

    #[test]
    fn empty_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.mp4");
        std::fs::write(&path, b"").unwrap();
        let plan = CommandPlan {
            program: "ffmpeg".into(),
            tool: Tool::Ffmpeg,
            args: Vec::<OsString>::new(),
            output: path.clone(),
            expected_kind: MediaKind::Video,
            expected_ext: "mp4".into(),
            remux: false,
            used_hardware: false,
            notes: vec![],
            duration_hint: None,
        };
        assert!(validate_output(&path, &plan).is_err());
    }
}

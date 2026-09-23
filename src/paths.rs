use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub const APP_ID: &str = "com.shadowfetch.Convert";
pub const APP_NAME: &str = "Shadow Convert";
pub const APP_ICON: &str = "shadow-convert";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_WEBSITE: &str = "https://github.com/Shadowfetchapps/Shadow-Convert";
pub const BINARY_NAME: &str = "shadow-convert";

pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| {
        Error::user("Could not find the user configuration directory (XDG_CONFIG_HOME).")
    })?;
    Ok(base.join("shadow-convert"))
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("settings.json"))
}

pub fn cache_dir() -> Result<PathBuf> {
    let base = dirs::cache_dir().ok_or_else(|| {
        Error::user("Could not find the user cache directory (XDG_CACHE_HOME).")
    })?;
    Ok(base.join("shadow-convert"))
}

pub fn temp_root() -> Result<PathBuf> {
    Ok(cache_dir()?.join("tmp"))
}

pub fn default_output_dir() -> Option<PathBuf> {
    dirs::video_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Videos")))
        .map(|videos| videos.join("Shadow Convert"))
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|err| {
        Error::detailed(
            format!("Could not create folder {}", path.display()),
            err.to_string(),
        )
    })
}

/// Unique path next to `dir` that never overwrites an existing file.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let ext = ext.trim_start_matches('.');
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    if !candidate.exists() {
        return candidate;
    }
    for n in 2..10_000 {
        candidate = dir.join(format!("{stem} ({n}).{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}.{ext}", std::process::id()))
}

/// Never return the source path itself. If the candidate would collide with
/// the source or an existing file, add a suffix.
pub fn unique_output_path(source: &Path, dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut path = unique_path(dir, stem, ext);
    if paths_equal(&path, source) {
        path = unique_path(dir, &format!("{stem} (converted)"), ext);
    }
    path
}

pub fn paths_equal(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(aa), Ok(bb)) => aa == bb,
        _ => a == b,
    }
}

pub fn display_home_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(&home) {
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}

pub fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn unique_path_avoids_existing() {
        let dir = tempfile::tempdir().unwrap();
        let first = unique_path(dir.path(), "clip", "mp4");
        fs::write(&first, b"a").unwrap();
        let second = unique_path(dir.path(), "clip", "mp4");
        assert_ne!(first, second);
        assert!(second
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("clip (2)"));
    }

    #[test]
    fn unique_output_never_equals_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("clip.mp4");
        fs::write(&source, b"a").unwrap();
        let out = unique_output_path(&source, dir.path(), "clip", "mp4");
        assert!(!paths_equal(&out, &source));
    }
}

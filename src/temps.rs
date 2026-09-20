use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use uuid::Uuid;

use crate::error::{Error, Result};
use crate::paths;

const MARKER: &str = ".shadow-convert-temp";
const STALE_AFTER: Duration = Duration::from_secs(6 * 60 * 60);

pub struct TempJob {
    pub dir: PathBuf,
}

impl TempJob {
    pub fn create() -> Result<Self> {
        let root = paths::temp_root()?;
        paths::ensure_dir(&root)?;
        cleanup_stale()?;
        let dir = root.join(format!("job-{}-{}", std::process::id(), Uuid::new_v4()));
        paths::ensure_dir(&dir)?;
        fs::write(dir.join(MARKER), b"shadow-convert")?;
        Ok(Self { dir })
    }

    pub fn child(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

impl Drop for TempJob {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Remove only directories that contain our marker and are older than STALE_AFTER.
pub fn cleanup_stale() -> Result<()> {
    let root = paths::temp_root()?;
    if !root.is_dir() {
        return Ok(());
    }
    let now = SystemTime::now();
    let entries = fs::read_dir(&root).map_err(|err| {
        Error::detailed("Could not inspect temporary files.", err.to_string())
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join(MARKER).is_file() {
            continue;
        }
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if now.duration_since(modified).unwrap_or_default() > STALE_AFTER {
            let _ = fs::remove_dir_all(&path);
        }
    }
    Ok(())
}

pub fn is_our_temp(path: &Path) -> bool {
    path.join(MARKER).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_cleans_marker() {
        let job = TempJob::create().unwrap();
        assert!(is_our_temp(&job.dir));
        let path = job.dir.clone();
        drop(job);
        assert!(!path.exists());
    }
}

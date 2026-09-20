pub mod detect;
pub mod error;
pub mod ffmpeg;
pub mod hw;
pub mod paths;
pub mod plan;
pub mod probe;
pub mod settings;
pub mod temps;
pub mod validate;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub use error::{Error, Result};
pub use plan::{
    estimate_size_bytes, AudioFormat, ImageFormat, JobKind, JobRequest, ResolutionTarget,
    VideoContainer,
};
pub use probe::Probe;
pub use settings::{Quality, Settings, Theme};

use crate::plan::CommandPlan;

pub fn probe_file(path: &std::path::Path) -> Result<Probe> {
    temps::cleanup_stale().ok();
    probe::probe(path)
}

pub fn plan_job(probe: &Probe, request: &JobRequest) -> Result<CommandPlan> {
    plan::build_plan(probe, request)
}

pub fn convert(
    probe: &Probe,
    request: &JobRequest,
    cancel: Arc<AtomicBool>,
    on_progress: impl FnMut(ffmpeg::Progress),
) -> Result<ffmpeg::RunResult> {
    let plan = plan::build_plan(probe, request)?;
    let result = ffmpeg::run_plan(&plan, cancel, on_progress)?;
    validate::validate_output(&result.output, &plan)?;
    Ok(result)
}

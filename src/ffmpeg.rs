use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::plan::{CommandPlan, Tool};

#[derive(Debug, Clone)]
pub struct Progress {
    pub ratio: f64,
    pub out_time_secs: Option<f64>,
    pub speed: Option<f64>,
    pub eta_secs: Option<f64>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub output: std::path::PathBuf,
    pub used_hardware: bool,
    pub remux: bool,
    pub notes: Vec<String>,
    pub elapsed: Duration,
}

pub fn run_plan(
    plan: &CommandPlan,
    cancel: Arc<AtomicBool>,
    mut on_progress: impl FnMut(Progress),
) -> Result<RunResult> {
    cancel.store(false, Ordering::SeqCst);
    let started = Instant::now();

    let mut cmd = Command::new(&plan.program);
    cmd.args(&plan.args);
    cmd.stdin(Stdio::null());
    if plan.tool == Tool::Ffmpeg {
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
    } else {
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn().map_err(|err| {
        Error::detailed(
            format!("Could not start {}.", display_tool(plan.tool)),
            err.to_string(),
        )
    })?;

    let result = if plan.tool == Tool::Ffmpeg {
        watch_ffmpeg(&mut child, plan, &cancel, &mut on_progress)
    } else {
        watch_simple(&mut child, plan, &cancel, &mut on_progress)
    };

    match result {
        Ok(()) => {
            if cancel.load(Ordering::SeqCst) {
                let _ = std::fs::remove_file(&plan.output);
                return Err(Error::user("Conversion was cancelled."));
            }
            if !plan.output.is_file() {
                return Err(Error::user(
                    "The conversion finished but no output file was created.",
                ));
            }
            Ok(RunResult {
                output: plan.output.clone(),
                used_hardware: plan.used_hardware,
                remux: plan.remux,
                notes: plan.notes.clone(),
                elapsed: started.elapsed(),
            })
        }
        Err(err) => {
            let _ = kill_group(&mut child);
            if plan.output.is_file() {
                if let Ok(meta) = std::fs::metadata(&plan.output) {
                    if meta.len() == 0 {
                        let _ = std::fs::remove_file(&plan.output);
                    }
                }
            }
            Err(err)
        }
    }
}

fn watch_ffmpeg(
    child: &mut Child,
    plan: &CommandPlan,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(Progress),
) -> Result<()> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::user("Could not read conversion progress."))?;
    let stderr = child.stderr.take();
    let stderr_handle = stderr.map(|pipe| {
        thread::spawn(move || {
            let mut text = String::new();
            let reader = BufReader::new(pipe);
            for line in reader.lines().flatten() {
                text.push_str(&line);
                text.push('\n');
                if text.len() > 64 * 1024 {
                    break;
                }
            }
            text
        })
    });

    let mut last_ui = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    let mut out_time = 0.0f64;
    let mut speed = None;
    let reader = BufReader::new(stdout);

    for line in reader.lines().flatten() {
        if cancel.load(Ordering::SeqCst) {
            let _ = kill_group(child);
            let _ = child.wait();
            return Err(Error::user("Conversion was cancelled."));
        }
        if let Some(value) = line.strip_prefix("out_time_ms=") {
            if let Ok(ms) = value.trim().parse::<u64>() {
                out_time = ms as f64 / 1000.0;
            }
        } else if let Some(value) = line.strip_prefix("out_time_us=") {
            if let Ok(us) = value.trim().parse::<u64>() {
                out_time = us as f64 / 1_000_000.0;
            }
        } else if let Some(value) = line.strip_prefix("speed=") {
            let trimmed = value.trim().trim_end_matches('x');
            speed = trimmed.parse().ok();
        } else if line.starts_with("progress=end") {
            break;
        }

        if last_ui.elapsed() >= Duration::from_millis(120) {
            last_ui = Instant::now();
            on_progress(make_progress(plan, out_time, speed));
        }
    }

    if cancel.load(Ordering::SeqCst) {
        let _ = kill_group(child);
        let _ = child.wait();
        return Err(Error::user("Conversion was cancelled."));
    }

    let status = child
        .wait()
        .map_err(|err| Error::detailed("The converter stopped unexpectedly.", err.to_string()))?;
    let stderr_text = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    if !status.success() {
        return Err(Error::detailed(
            "The conversion failed. The file may use an unsupported codec.",
            stderr_text,
        ));
    }
    on_progress(Progress {
        ratio: 1.0,
        out_time_secs: Some(out_time),
        speed,
        eta_secs: Some(0.0),
        message: "Finishing…".into(),
    });
    Ok(())
}

fn watch_simple(
    child: &mut Child,
    _plan: &CommandPlan,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(Progress),
) -> Result<()> {
    on_progress(Progress {
        ratio: 0.15,
        out_time_secs: None,
        speed: None,
        eta_secs: None,
        message: "Converting image…".into(),
    });
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = kill_group(child);
            let _ = child.wait();
            return Err(Error::user("Conversion was cancelled."));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stderr = String::new();
                if let Some(pipe) = child.stderr.as_mut() {
                    use std::io::Read;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                if !status.success() {
                    return Err(Error::detailed(
                        "The image conversion failed.",
                        stderr,
                    ));
                }
                on_progress(Progress {
                    ratio: 1.0,
                    out_time_secs: None,
                    speed: None,
                    eta_secs: Some(0.0),
                    message: "Finishing…".into(),
                });
                return Ok(());
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(err) => {
                return Err(Error::detailed(
                    "The converter stopped unexpectedly.",
                    err.to_string(),
                ));
            }
        }
    }
}

fn make_progress(plan: &CommandPlan, out_time: f64, speed: Option<f64>) -> Progress {
    let ratio = match plan.duration_hint {
        Some(total) if total > 0.05 => (out_time / total).clamp(0.0, 0.99),
        _ => 0.15,
    };
    let eta = match (plan.duration_hint, speed) {
        (Some(total), Some(sp)) if sp > 0.05 => Some(((total - out_time) / sp).max(0.0)),
        (Some(total), _) if total > out_time && out_time > 0.2 => {
            Some((total - out_time) * (1.0 / ratio.max(0.05)))
        }
        _ => None,
    };
    let message = match (plan.duration_hint, eta) {
        (Some(total), Some(eta)) => format!(
            "{} of {}  ·  ETA {}",
            crate::probe::format_duration(out_time),
            crate::probe::format_duration(total),
            crate::probe::format_duration(eta)
        ),
        (Some(total), None) => format!(
            "{} of {}",
            crate::probe::format_duration(out_time),
            crate::probe::format_duration(total)
        ),
        _ => "Working…".into(),
    };
    Progress {
        ratio,
        out_time_secs: Some(out_time),
        speed,
        eta_secs: eta,
        message,
    }
}

fn kill_group(child: &mut Child) -> Result<()> {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        thread::sleep(Duration::from_millis(80));
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    Ok(())
}

fn display_tool(tool: Tool) -> &'static str {
    match tool {
        Tool::Ffmpeg => "FFmpeg",
        Tool::Convert => "ImageMagick",
    }
}

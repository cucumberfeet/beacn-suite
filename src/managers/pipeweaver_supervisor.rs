//! Runs Pipeweaver's daemon as a child of this app, so the Beacn Utility is the one
//! service, one tray icon and one launcher for both device control and audio routing.
//!
//! Enabled with `--with-pipeweaver` (or `BEACN_WITH_PIPEWEAVER=1`). The daemon is started
//! with `--background --no-tray` (the `--no-tray` flag comes from the beacn-suite branch of
//! Pipeweaver; `PIPEWEAVER_NO_TRAY=1` is also set for the same purpose). If a Pipeweaver
//! daemon that we didn't start is already running, we leave it alone and check again later.

use crate::ManagerMessages;
use beacn_lib::flume::Receiver;
use log::{debug, info, warn};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use std::{env, fs};
use tokio::process::{Child, Command};
use tokio::time::sleep;

const DAEMON: &str = "pipeweaver-daemon";

pub fn enabled(flag: bool) -> bool {
    flag || env::var("BEACN_WITH_PIPEWEAVER").is_ok_and(|v| !v.is_empty() && v != "0")
}

fn find_daemon() -> Option<PathBuf> {
    if let Ok(path) = env::var("BEACN_PIPEWEAVER_DAEMON") {
        return Some(PathBuf::from(path));
    }
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(DAEMON);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|p| p.join(DAEMON))
            .find(|p| p.is_file())
    })
}

/// Is there a pipeweaver-daemon running that isn't our child?
fn foreign_daemon_running(own: Option<u32>) -> bool {
    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    let my_uid = fs::metadata("/proc/self").map(|m| {
        use std::os::unix::fs::MetadataExt;
        m.uid()
    });
    entries.flatten().any(|e| {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
            return false;
        };
        if Some(pid) == own {
            return false;
        }
        let same_user = match (&my_uid, e.metadata()) {
            (Ok(uid), Ok(meta)) => {
                use std::os::unix::fs::MetadataExt;
                meta.uid() == *uid
            }
            _ => false,
        };
        // comm is truncated to 15 characters: "pipeweaver-daem"
        same_user
            && fs::read_to_string(e.path().join("comm"))
                .is_ok_and(|c| DAEMON.starts_with(c.trim()) && c.trim().len() >= 12)
    })
}

fn spawn(path: &PathBuf) -> std::io::Result<Child> {
    let mut cmd = Command::new(path);
    cmd.args(["--background", "--no-tray"])
        .env("PIPEWEAVER_NO_TRAY", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    // If we die without cleaning up, take the daemon with us.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    cmd.spawn()
}

async fn stop(child: &mut Child) {
    if let Some(pid) = child.id() {
        debug!("Asking Pipeweaver ({pid}) to stop");
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        if tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .is_ok()
        {
            return;
        }
        warn!("Pipeweaver didn't stop in time, killing it");
    }
    let _ = child.kill().await;
}

pub async fn supervise(rx: Receiver<ManagerMessages>) {
    let Some(path) = find_daemon() else {
        warn!("--with-pipeweaver given, but {DAEMON} wasn't found next to this binary or on PATH");
        let _ = rx.recv_async().await;
        return;
    };
    info!("Supervising Pipeweaver at {}", path.display());

    let mut child: Option<Child> = None;
    let mut backoff = Duration::from_secs(1);
    let mut started_at = Instant::now();

    loop {
        if child.is_none() {
            if foreign_daemon_running(None) {
                debug!("A Pipeweaver daemon is already running outside of us, not starting one");
            } else {
                match spawn(&path) {
                    Ok(c) => {
                        info!("Started Pipeweaver (pid {:?})", c.id());
                        started_at = Instant::now();
                        child = Some(c);
                    }
                    Err(e) => warn!("Failed to start Pipeweaver: {e}"),
                }
            }
        }

        match child.as_mut() {
            Some(c) => {
                tokio::select! {
                    msg = rx.recv_async() => {
                        if matches!(msg, Ok(ManagerMessages::Quit) | Err(_)) {
                            stop(c).await;
                            break;
                        }
                    }
                    status = c.wait() => {
                        warn!("Pipeweaver exited ({status:?})");
                        child = None;
                        // Reset the backoff if it ran for a decent while
                        if started_at.elapsed() > Duration::from_secs(60) {
                            backoff = Duration::from_secs(1);
                        }
                        tokio::select! {
                            msg = rx.recv_async() => {
                                if matches!(msg, Ok(ManagerMessages::Quit) | Err(_)) { break; }
                            }
                            _ = sleep(backoff) => {}
                        }
                        backoff = (backoff * 2).min(Duration::from_secs(30));
                    }
                }
            }
            None => {
                tokio::select! {
                    msg = rx.recv_async() => {
                        if matches!(msg, Ok(ManagerMessages::Quit) | Err(_)) { break; }
                    }
                    _ = sleep(Duration::from_secs(10)) => {}
                }
            }
        }
    }
    debug!("Pipeweaver supervisor stopped");
}

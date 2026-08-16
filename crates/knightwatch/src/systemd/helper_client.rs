use libc::{STDIN_FILENO, TCSANOW, tcgetattr, tcsetattr, termios};
use std::{mem::MaybeUninit, path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    process::{Child, Command},
    sync::Mutex,
};

use kw_types::{
    systemd::ServiceAction,
    systemd_helper::{HelperRequest, HelperResponse},
};

use crate::prelude::*;

/// Client handle to the privileged `kw-systemd-helper` process.
/// Lives for as long as the server does; owns the child process and cleans
/// up its socket file on drop.
pub struct SystemdHelperClient {
    stream: Mutex<BufReader<UnixStream>>,
    socket_path: PathBuf,
    _child: Child,
}

impl SystemdHelperClient {
    /// Spawns the helper via `pkexec`, prompting for authentication once,
    /// and connects to its socket. Returns `Ok(None)` if the user cancelled
    /// or denied the polkit prompt — treated as "commands stay disabled",
    /// not a fatal error.
    #[allow(clippy::print_stderr)]
    pub async fn spawn() -> Result<Option<Self>> {
        let pid = std::process::id();
        let socket_path = helper_socket_path(pid);
        let uid = current_uid();
        let helper_bin = locate_helper_binary()?;

        let helper_args = [
            "--socket".to_string(),
            socket_path.display().to_string(),
            "--uid".to_string(),
            uid.to_string(),
            "--watch-pid".to_string(),
            pid.to_string(),
        ];

        let elevators: &[&str] = if Self::is_wsl() {
            &["sudo"]
        } else {
            &["pkexec", "sudo"]
        };

        crate::telemetry::pause_logging();
        eprintln!("\n🔐 Waiting for authentication to enable systemd commands...");

        let saved_termios = Self::save_termios();
        let result = Self::spawn_inner(elevators, &helper_bin, &helper_args, &socket_path).await;
        // Restore regardless of success/failure/timeout — any of those paths
        // can leave the tty in whatever state the elevator's password prompt
        // put it in.
        if let Some(term) = &saved_termios {
            Self::restore_termios(term);
        }

        crate::telemetry::resume_logging();
        result
    }

    async fn spawn_inner(
        elevators: &[&str],
        helper_bin: &PathBuf,
        helper_args: &[String],
        socket_path: &PathBuf,
    ) -> Result<Option<Self>> {
        for &elevator in elevators {
            let mut cmd = Command::new(elevator);
            cmd.arg(helper_bin).args(helper_args).kill_on_drop(true);

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    return Err(Error::Systemd(format!(
                        "failed to spawn kw-systemd-helper via {elevator}: {e}"
                    )));
                }
            };

            let wait_for_socket = async {
                loop {
                    if socket_path.exists() {
                        return Ok(true);
                    }
                    if let Some(status) = child.try_wait().map_err(|e| {
                        Error::Systemd(format!("failed to poll kw-systemd-helper: {e}"))
                    })? {
                        warn!(
                            ?status,
                            elevator, "kw-systemd-helper exited before starting"
                        );
                        return Ok(false);
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            };

            match tokio::time::timeout(Duration::from_mins(1), wait_for_socket).await {
                Ok(Ok(true)) => {
                    let stream = UnixStream::connect(&socket_path).await.map_err(|e| {
                        Error::Systemd(format!("failed to connect to helper socket: {e}"))
                    })?;
                    return Ok(Some(Self {
                        stream: Mutex::new(BufReader::new(stream)),
                        socket_path: socket_path.clone(),
                        _child: child,
                    }));
                }
                Ok(Ok(false)) => {}
                Ok(Err(e)) => return Err(e),
                Err(_elapsed) => {
                    warn!(elevator, "timed out waiting for systemd command helper");
                    let _ = child.start_kill();
                }
            }
        }

        Ok(None)
    }

    /// Snapshot the controlling terminal's current mode, if stdin is a tty.
    /// `None` if there isn't one (e.g. running non-interactively) — nothing
    /// to restore in that case.
    fn save_termios() -> Option<termios> {
        unsafe {
            let mut term = MaybeUninit::<termios>::uninit();
            if tcgetattr(STDIN_FILENO, term.as_mut_ptr()) == 0 {
                Some(term.assume_init())
            } else {
                None
            }
        }
    }

    /// Force the tty back to whatever `save_termios` captured. sudo/pkexec's
    /// password-prompt handling doesn't reliably restore ISIG/ECHO itself
    /// (especially under WSL's pty layer), which is why a single Ctrl+C
    /// stops reaching us as SIGINT after authenticating.
    fn restore_termios(term: &termios) {
        unsafe {
            tcsetattr(STDIN_FILENO, TCSANOW, std::ptr::from_ref::<termios>(term));
        }
    }

    fn is_wsl() -> bool {
        std::env::var_os("WSL_DISTRO_NAME").is_some()
            || std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .is_ok_and(|s| s.to_lowercase().contains("microsoft"))
    }

    pub async fn control(&self, unit_name: &str, action: ServiceAction) -> Result<()> {
        let request = HelperRequest::Control {
            unit_name: unit_name.to_string(),
            action,
        };
        match self.roundtrip(&request).await? {
            HelperResponse::Ok => Ok(()),
            HelperResponse::Err { message } => Err(Error::Systemd(message)),
            HelperResponse::Pong => Err(Error::Systemd("unexpected response from helper".into())),
        }
    }

    async fn roundtrip(&self, request: &HelperRequest) -> Result<HelperResponse> {
        let mut line = serde_json::to_string(request)
            .map_err(|e| Error::Systemd(format!("failed to encode helper request: {e}")))?;
        line.push('\n');

        let mut guard = self.stream.lock().await;
        guard
            .get_mut()
            .write_all(line.as_bytes())
            .await
            .map_err(|e| Error::Systemd(format!("failed to write to helper: {e}")))?;

        let mut response_line = String::new();
        let n = guard
            .read_line(&mut response_line)
            .await
            .map_err(|e| Error::Systemd(format!("failed to read from helper: {e}")))?;
        drop(guard);
        if n == 0 {
            return Err(Error::Systemd("helper closed the connection".into()));
        }
        serde_json::from_str(&response_line)
            .map_err(|e| Error::Systemd(format!("failed to decode helper response: {e}")))
    }
}

#[must_use]
fn current_uid() -> u32 {
    // SAFETY: getuid() has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

/// Directory used for the systemd-helper's Unix domain socket.
/// Prefers `XDG_RUNTIME_DIR` (tmpfs, mode 0700, cleared on logout);
/// falls back to a per-uid dir under `/tmp` when it's unset.
fn helper_runtime_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join("knightwatch");
    }
    PathBuf::from(format!("/tmp/knightwatch-{}", current_uid()))
}

/// One socket per server instance (by pid) so multiple instances / restarts
/// never collide on a stale path.
#[must_use]
pub fn helper_socket_path(pid: u32) -> PathBuf {
    helper_runtime_dir().join(format!("systemd-helper-{pid}.sock"))
}

impl Drop for SystemdHelperClient {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
        // `kill_on_drop(true)` on the Command handles killing the child.
    }
}

fn locate_helper_binary() -> Result<PathBuf> {
    let exe = std::env::current_exe()
        .map_err(|e| Error::Systemd(format!("failed to resolve current exe: {e}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| Error::Systemd("current exe has no parent directory".into()))?;
    let candidate = dir.join("kw-systemd-helper");
    if candidate.exists() {
        return Ok(candidate);
    }
    // Dev fallback: rely on PATH (e.g. `cargo run` setups).
    Ok(PathBuf::from("kw-systemd-helper"))
}

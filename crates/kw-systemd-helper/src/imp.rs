use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use std::{
    ffi::CString, fs, os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc, time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream, unix::OwnedWriteHalf},
    sync::Notify,
};
use zbus::Connection;

use kw_types::{
    systemd::ServiceAction,
    systemd_helper::{HelperRequest, HelperResponse},
};

use super::proxies::{SystemdManagerProxy, SystemdUnitProxy};

/// Privileged helper for knightwatch: the only part of the toolchain that
/// runs as root. Spawned once via `pkexec` by the unprivileged server and
/// talked to over a Unix domain socket for the lifetime of that process.
#[derive(Parser)]
pub struct Args {
    /// Path to bind the Unix domain socket at (parent creates the dir).
    #[arg(long)]
    pub socket: PathBuf,
    /// UID of the unprivileged server process — only this uid may connect.
    #[arg(long)]
    pub uid: u32,
    /// PID to watch; the helper exits if this process disappears.
    #[arg(long)]
    pub watch_pid: u32,
}

pub async fn run(args: Args) -> anyhow::Result<()> {
    let conn = Connection::system().await?;

    if let Some(parent) = args.socket.parent() {
        fs::create_dir_all(parent)?;
    }
    let _ = fs::remove_file(&args.socket);

    let listener = UnixListener::bind(&args.socket)?;
    fs::set_permissions(&args.socket, fs::Permissions::from_mode(0o600))?;
    chown_to_uid(&args.socket, args.uid)?;

    tracing::info!(socket = %args.socket.display(), "kw-systemd-helper listening");

    let shutdown = Arc::new(Notify::new());
    spawn_parent_watchdog(args.watch_pid, args.socket.clone(), shutdown.clone());

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                if !peer_is_authorized(&stream, args.uid) {
                    tracing::warn!("rejected connection from unauthorized peer");
                    continue;
                }
                let conn = conn.clone();
                let socket_path = args.socket.clone();
                let shutdown = shutdown.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, &conn, &socket_path, shutdown).await {
                        tracing::debug!(?e, "connection closed");
                    }
                });
            }
            () = shutdown.notified() => {
                tracing::info!("shutdown requested, exiting");
                break;
            }
        }
    }

    Ok(())
}

fn chown_to_uid(path: &std::path::Path, uid: u32) -> Result<()> {
    let c_path = CString::new(path.as_os_str().as_encoded_bytes())?;
    // SAFETY: c_path is a valid, NUL-terminated path we just created;
    // gid=u32::MAX leaves the group unchanged.
    let rc = unsafe { libc::chown(c_path.as_ptr(), uid, u32::MAX) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn spawn_parent_watchdog(watch_pid: u32, socket_path: PathBuf, shutdown: Arc<Notify>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let alive = unsafe { libc::kill(watch_pid.cast_signed(), 0) == 0 };
            if !alive {
                tracing::warn!("parent process gone, shutting down");
                let _ = fs::remove_file(&socket_path);
                shutdown.notify_one();
                return;
            }
        }
    });
}

fn peer_is_authorized(stream: &UnixStream, expected_uid: u32) -> bool {
    stream
        .peer_cred()
        .is_ok_and(|cred| cred.uid() == expected_uid)
}

async fn handle_connection(
    stream: UnixStream,
    conn: &Connection,
    socket_path: &PathBuf,
    shutdown: Arc<Notify>,
) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                send(
                    &mut write_half,
                    &HelperResponse::Err {
                        message: format!("bad request: {e}"),
                    },
                )
                .await?;
                continue;
            }
        };

        match request {
            HelperRequest::Ping => send(&mut write_half, &HelperResponse::Pong).await?,
            HelperRequest::Shutdown => {
                let _ = fs::remove_file(socket_path);
                shutdown.notify_one();
                return Ok(());
            }
            HelperRequest::Control { unit_name, action } => {
                let resp = match control_unit(conn, &unit_name, action).await {
                    Ok(()) => HelperResponse::Ok,
                    Err(e) => HelperResponse::Err {
                        message: e.to_string(),
                    },
                };
                send(&mut write_half, &resp).await?;
            }
        }
    }
    Ok(())
}

async fn send(write_half: &mut OwnedWriteHalf, resp: &HelperResponse) -> Result<()> {
    let mut line = serde_json::to_string(resp)?;
    line.push('\n');
    write_half.write_all(line.as_bytes()).await?;
    Ok(())
}

async fn control_unit(conn: &Connection, unit_name: &str, action: ServiceAction) -> Result<()> {
    let manager = SystemdManagerProxy::new(conn).await?;

    // Subscribe BEFORE issuing the call, so we can't miss a fast completion.
    let mut job_removed = manager.receive_job_removed().await?;

    let mode = "replace";
    let job_path = match action {
        ServiceAction::Start => manager.start_unit(unit_name, mode).await?,
        ServiceAction::Stop => manager.stop_unit(unit_name, mode).await?,
        ServiceAction::Restart => manager.restart_unit(unit_name, mode).await?,
        ServiceAction::Reload => manager.reload_unit(unit_name, mode).await?,
    };

    let result = loop {
        let Some(signal) =
            tokio::time::timeout(std::time::Duration::from_secs(30), job_removed.next())
                .await
                .map_err(|_| anyhow::anyhow!("timed out waiting for systemd job to finish"))?
        else {
            anyhow::bail!("job signal stream ended unexpectedly");
        };

        let args = signal.args()?;
        if args.job() == &job_path {
            break args.result().clone();
        }
        // A JobRemoved for some other unit's job — keep waiting for ours.
    };

    match result.as_str() {
        "done" => Ok(()),
        "canceled" => {
            // Superseded by a later job (e.g. bus-activation bounced it
            // straight back). Not a failure of our call, but worth saying so.
            anyhow::bail!(
                "job for {unit_name} was superseded by another job before it completed \
                 (often caused by D-Bus/socket activation restarting the unit)"
            )
        }
        other => {
            let reason = fetch_failure_reason(conn, unit_name).await;
            anyhow::bail!("{unit_name} {action:?} finished with result '{other}'{reason}")
        }
    }
}

async fn fetch_failure_reason(conn: &Connection, unit_name: &str) -> String {
    let Ok(manager) = SystemdManagerProxy::new(conn).await else {
        return String::new();
    };
    let Ok(unit_path) = manager.get_unit(unit_name).await else {
        return String::new();
    };
    let Ok(builder) = SystemdUnitProxy::builder(conn).path(unit_path) else {
        return String::new();
    };
    let Ok(unit_proxy) = builder.build().await else {
        return String::new();
    };
    let active = unit_proxy.active_state().await.unwrap_or_default();
    let sub = unit_proxy.sub_state().await.unwrap_or_default();
    format!(" (active_state={active}, sub_state={sub})")
}

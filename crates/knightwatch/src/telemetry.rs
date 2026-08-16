#![allow(unused)]

use std::io::{self, Write};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, MakeWriter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use crate::prelude::*;

#[derive(Clone, Default)]
pub struct GatedWriter {
    paused: Arc<AtomicBool>,
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl GatedWriter {
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        let mut buf = self
            .buffer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !buf.is_empty() {
            let _ = io::stdout().write_all(&buf);
            let _ = io::stdout().flush();
            buf.clear();
        }
    }
}

impl Write for GatedWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let normalized = normalize_newlines(data);
        if self.paused.load(Ordering::SeqCst) {
            self.buffer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(&normalized);
        } else {
            io::stdout().write_all(&normalized)?;
        }
        // Report the ORIGINAL length consumed, not the normalized one —
        // callers (tracing's fmt writer) expect write() to return how much
        // of `data` was accepted, and the byte counts diverge once we've
        // inserted extra `\r`s.
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()
    }
}

/// Insert an explicit `\r` before every `\n` that isn't already preceded by
/// one. Terminal `ONLCR` handling can end up unreliable for the rest of a
/// session after a `sudo`/`pkexec` password prompt (especially under WSL),
/// so we stop depending on it entirely rather than just patching the
/// buffered-flush window.
fn normalize_newlines(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut prev = None;
    for &b in data {
        if b == b'\n' && prev != Some(b'\r') {
            out.push(b'\r');
        }
        out.push(b);
        prev = Some(b);
    }
    out
}

impl<'a> MakeWriter<'a> for GatedWriter {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

static GATED_WRITER: OnceLock<GatedWriter> = OnceLock::new();

pub fn init_tracing() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info,zbus=off"))
        .map_err(|e| Error::Other(format!("Failed to initialize env filter: {e}")))?;
    let writer = GatedWriter::default();
    let _ = GATED_WRITER.set(writer.clone());
    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true)
                .with_span_events(fmt::format::FmtSpan::NEW)
                .with_writer(writer),
        )
        .init();
    Ok(())
}

/// Buffers all log output instead of dropping it, e.g. while a `sudo`/`pkexec`
/// password prompt owns the terminal. Pair with [`resume_logging`].
pub fn pause_logging() {
    if let Some(w) = GATED_WRITER.get() {
        w.pause();
    }
}

/// Flushes anything buffered during [`pause_logging`] and resumes normal output.
pub fn resume_logging() {
    if let Some(w) = GATED_WRITER.get() {
        w.resume();
    }
}

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init_tracing() -> Result<(), crate::errors::Error> {
    let env_filter: EnvFilter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info,zbus=off"))
        .map_err(|e: tracing_subscriber::filter::ParseError| {
            crate::errors::Error::Other(format!("Failed to initialize env filter: {}", e))
        })?;
    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true)
                .with_span_events(fmt::format::FmtSpan::NEW),
        )
        .init();
    Ok(())
}

#[cfg(unix)]
mod imp;
#[cfg(unix)]
mod proxies;

#[cfg(unix)]
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = <imp::Args as clap::Parser>::parse();
    if let Err(e) = imp::run(args).await {
        tracing::error!(?e, "kw-systemd-helper exited with error");
        std::process::exit(1);
    }
}

#[cfg(not(unix))]
#[allow(clippy::print_stderr)]
fn main() {
    eprintln!(
        "kw-systemd-helper only supports Unix-like systems (requires D-Bus, polkit, and Unix domain sockets)."
    );
    std::process::exit(1);
}

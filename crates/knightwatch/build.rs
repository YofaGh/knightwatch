#![allow(clippy::expect_used, clippy::panic, clippy::print_stderr)]

fn main() {
    let profile = std::env::var("PROFILE").unwrap_or_default();
    let is_dist = std::env::var("CARGO_DIST_VERSION").is_ok();
    if profile != "release" && !is_dist {
        return;
    }

    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let workspace = std::env::var("CARGO_WORKSPACE_DIR")
        .expect("CARGO_WORKSPACE_DIR should be set by the cargo workspace");
    let dashboard = format!("{workspace}/dashboard");

    println!("cargo:rerun-if-changed={dashboard}/src");
    println!("cargo:rerun-if-changed={dashboard}/package.json");
    println!("cargo:rerun-if-changed={dashboard}/svelte.config.js");
    println!("cargo:rerun-if-changed={dashboard}/vite.config.js");

    let node_modules = format!("{dashboard}/node_modules");
    if std::env::var("CI").is_ok()
        && std::path::Path::new(&node_modules).exists()
        && let Err(e) = std::fs::remove_dir_all(&node_modules)
    {
        eprintln!("Warning: could not remove node_modules: {e}");
    }

    let lockfile = format!("{dashboard}/package-lock.json");
    if std::env::var("CI").is_ok()
        && std::path::Path::new(&lockfile).exists()
        && let Err(e) = std::fs::remove_file(&lockfile)
    {
        eprintln!("Warning: could not remove package-lock.json: {e}");
    }

    run_npm(npm, &["install"], &dashboard);
    run_npm(npm, &["run", "build"], &dashboard);
}

fn run_npm(npm: &str, args: &[&str], cwd: &str) {
    let status = std::process::Command::new(npm)
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap_or_else(|e| panic!("Failed to spawn npm {args:?}: {e}"));

    assert!(
        status.success(),
        "npm {args:?} failed with status: {status}"
    );
}

#[cfg(not(debug_assertions))]
#[derive(rust_embed::Embed)]
#[folder = "$CARGO_WORKSPACE_DIR/dashboard/dist/"]
pub struct DashboardAssets;

pub struct Vite {
    pub child_process: std::process::Child,
}

impl Vite {
    pub fn stop(mut self) {
        let _ = self.child_process.kill();
        let _ = self.child_process.wait();
        tracing::info!("Shutdown vite");
    }
}

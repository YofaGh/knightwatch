use std::process::Child;

use crate::prelude::{info, warn};

#[cfg(not(debug_assertions))]
#[derive(rust_embed::Embed)]
#[folder = "$CARGO_WORKSPACE_DIR/dashboard/dist/"]
pub struct DashboardAssets;

pub struct Vite {
    pub child_process: Child,
}

impl Vite {
    #[cfg(debug_assertions)]
    pub const fn new(child_process: Child) -> Self {
        Self { child_process }
    }

    pub fn stop(mut self) {
        if let Err(e) = self.child_process.kill() {
            warn!("Failed to kill vite process: {e}");
        }
        let _ = self.child_process.wait();
        info!("Shutdown vite");
    }
}

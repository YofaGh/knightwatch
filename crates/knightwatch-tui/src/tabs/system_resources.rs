pub struct SystemResourcesTab {}

impl Default for SystemResourcesTab {
    fn default() -> Self {
        Self::new()
    }
}

impl super::Tab for SystemResourcesTab {
    fn name(&self) -> &'static str {
        "System Resources"
    }
}

impl SystemResourcesTab {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct DockerTab {}

impl DockerTab {
    pub fn new() -> Self {
        Self {}
    }
}

impl super::Tab for DockerTab {
    fn name(&self) -> &'static str {
        "Docker"
    }
}

impl Default for DockerTab {
    fn default() -> Self {
        Self::new()
    }
}

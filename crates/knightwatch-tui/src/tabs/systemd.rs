pub struct SystemdTab {}

impl Default for SystemdTab {
    fn default() -> Self {
        Self::new()
    }
}

impl super::Tab for SystemdTab {
    fn name(&self) -> &'static str {
        "Systemd"
    }
}

impl SystemdTab {
    pub fn new() -> Self {
        Self {}
    }
}

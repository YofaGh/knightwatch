pub struct ProcessesTab {}

impl Default for ProcessesTab {
    fn default() -> Self {
        Self::new()
    }
}

impl super::Tab for ProcessesTab {
    fn name(&self) -> &'static str {
        "Processes"
    }
}

impl ProcessesTab {
    pub fn new() -> Self {
        Self {}
    }
}

pub mod api;
pub mod docker;
pub mod polling;
pub mod process;
pub mod resources;
pub mod screen;
pub mod systemd;
pub mod systemd_helper;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Info {
    pub auth_enabled: bool,
    pub shutdown_enabled: bool,
    pub blind: bool,
    pub pid: Vec<u32>,
    pub top_processes: bool,
    pub limit_processes: usize,
    pub telegram_bot: bool,
    pub system_resources: bool,
    pub systemd: bool,
    pub docker: bool,
    pub allow_process_commands: bool,
    pub allow_screen_commands: bool,
    pub allow_system_resources_commands: bool,
    pub allow_systemd_commands: bool,
    pub allow_docker_commands: bool,
}

impl std::fmt::Display for Info {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let pids: Vec<String> = self
            .pid
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        writeln!(f, "auth enabled:    {}", self.auth_enabled)?;
        writeln!(f, "shutdown enabled:    {}", self.shutdown_enabled)?;
        writeln!(f, "blind:           {}", self.blind)?;
        writeln!(
            f,
            "tracked PIDs:    {}",
            if pids.is_empty() {
                "none".into()
            } else {
                pids.join(", ")
            }
        )?;
        writeln!(
            f,
            "top processes:   {}  (limit: {})",
            self.top_processes, self.limit_processes
        )?;
        writeln!(f, "telegram bot:    {}", self.telegram_bot)?;
        writeln!(
            f,
            "features:        system_resources={} systemd={} docker={}",
            self.system_resources, self.systemd, self.docker
        )?;
        writeln!(
            f,
            "commands:        process={} screen={} resources={} systemd={} docker={}",
            self.allow_process_commands,
            self.allow_screen_commands,
            self.allow_system_resources_commands,
            self.allow_systemd_commands,
            self.allow_docker_commands
        )
    }
}

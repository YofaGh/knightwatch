use super::utils::escape_mdv2;

#[derive(teloxide::utils::command::BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
pub enum Command {
    #[command(description = "Start the bot and show the main menu")]
    Start,
    #[command(description = "Show the main menu")]
    Menu,
    #[command(description = "Show this help message")]
    Help,
    #[command(description = "Get Screenshot of all monitors")]
    Screenshot,
    #[command(description = "Get Process Info")]
    Process,
    #[command(description = "Get Top Processes Info")]
    TopProcesses,
    #[command(description = "Stop Knight Watch")]
    StopKnightWatch,
}

pub struct TelegramDisplay<'a, T>(pub &'a T);

impl<'a> std::fmt::Display for TelegramDisplay<'a, crate::process_tracker::structs::ProcessInfo> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self.0;
        write!(
            f,
            "🔹 *{name}* `(PID {pid})`\n   ├ State: `{state}`\n   ├ CPU: `{cpu:.1}%`\n   └ Mem: `{mem}`",
            pid = s.pid,
            name = escape_mdv2(&s.name),
            state = escape_mdv2(&s.state),
            cpu = s.cpu_usage,
            mem = escape_mdv2(&s.memory_human),
        )?;
        #[cfg(target_os = "linux")]
        {
            if let Some(cwd) = &s.cwd {
                write!(f, "\n   ├ CWD: `{}`", escape_mdv2(cwd))?;
            }
            write!(f, "\n   ├ FDs: `{}`", s.open_fds)?;
            if let Some(io) = &s.io_stats {
                write!(
                    f,
                    "\n   ├ I/O Read: `{}` / Write: `{}`",
                    escape_mdv2(&io.read_bytes.to_string()),
                    escape_mdv2(&io.write_bytes.to_string()),
                )?;
            }
            if !s.cmdline.is_empty() {
                let cmd = s.cmdline.join(" ");
                write!(f, "\n   └ CMD: `{}`", escape_mdv2(&cmd))?;
            }
        }
        Ok(())
    }
}

impl<'a> std::fmt::Display for TelegramDisplay<'a, crate::process_tracker::structs::ProcessTree> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let t = self.0; // Header
        let status_icon = if t.work_done { "✅" } else { "⏳" };
        writeln!(f, "{status_icon} *Process Tree*\n🕐 `{}`\n", t.timestamp)?;

        // Root process
        writeln!(f, "*Root Process*")?;
        match &t.root {
            Some(root) => writeln!(f, "{}", TelegramDisplay(root))?,
            None => writeln!(f, "_No root process_")?,
        }

        // Children
        if t.child_count == 0 {
            writeln!(f, "\n*Children:* _none_")?;
        } else {
            writeln!(f, "\n*Children* ({}):", t.child_count)?;
            for child in &t.children {
                writeln!(f, "{}\n", TelegramDisplay(child))?;
            }
        }
        Ok(())
    }
}

#![allow(clippy::print_stdout, clippy::print_stderr)]

use clap::Parser;
use rustyline::{
    Completer, Helper, Hinter, Validator, completion::Pair, error::ReadlineError,
    hint::HistoryHinter, validate::MatchingBracketValidator,
};
use std::borrow::Cow;

use crate::colors::{CYAN, DIM, RED, RESET, YELLOW};

/// All top-level subcommand names, used for tab-completion.
const SUBCOMMANDS: &[&str] = &[
    "health",
    "info",
    "shutdown",
    "login",
    "logout",
    "screenshot",
    "root-pids",
    "process-tree",
    "process-root",
    "process-children",
    "process-status",
    "process-is-done",
    "process-trees",
    "top-processes",
    "supported-signals",
    "kill-process",
    "kill-tree",
    "track-pid",
    "untrack-pid",
    "process-poll-pause",
    "process-poll-resume",
    "process-poll-interval",
    "screen-poll-pause",
    "screen-poll-resume",
    "screen-poll-interval",
    "system",
    "cpu",
    "memory",
    "disks",
    "networks",
    "gpus",
    "battery",
    "host-info",
    "temperatures",
    "alarms",
    "set-thresholds",
    "set-refresh-mask",
    "resources-poll-pause",
    "resources-poll-resume",
    "resources-poll-interval",
    "systemd",
    "unit",
    "units-by-state",
    "failed-units",
    "systemd-poll-pause",
    "systemd-poll-resume",
    "systemd-poll-interval",
    "docker-containers",
    "container",
    "top-containers",
    "stop-container",
    "kill-container",
    "start-container",
    "restart-container",
    "pause-container",
    "unpause-container",
    "docker-poll-pause",
    "docker-poll-resume",
    "docker-poll-interval",
    // meta
    "help",
    "exit",
    "quit",
];

#[derive(Helper, Completer, Hinter, Validator)]
struct ReplHelper {
    #[rustyline(Completer)]
    completer: SubcommandCompleter,
    #[rustyline(Hinter)]
    hinter: HistoryHinter,
    #[rustyline(Validator)]
    validator: MatchingBracketValidator,
}

impl rustyline::highlight::Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Borrowed(line)
    }
    fn highlight_char(
        &self,
        _line: &str,
        _pos: usize,
        _forced: rustyline::highlight::CmdKind,
    ) -> bool {
        false
    }
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        // Bold cyan prompt
        Cow::Owned(format!("{CYAN}{prompt}{RESET}"))
    }
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        // Dim grey inline hint
        Cow::Owned(format!("{DIM}{hint}{RESET}"))
    }
}

struct SubcommandCompleter;

impl rustyline::completion::Completer for SubcommandCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Only complete the first token
        let safe_pos = (0..=pos)
            .rev()
            .find(|&i| line.is_char_boundary(i))
            .unwrap_or(0);
        let prefix = line.get(..safe_pos).unwrap_or("");
        let word_start = prefix.rfind(' ').map_or(0, |i| i.saturating_add(1));
        let word = prefix.get(word_start..).unwrap_or("");
        let has_space = prefix.contains(' ');

        if has_space {
            // Don't complete arguments — just return nothing
            return Ok((pos, vec![]));
        }

        let candidates = SUBCOMMANDS
            .iter()
            .filter(|&&s| s.starts_with(word))
            .map(|&s| Pair {
                display: s.to_owned(),
                replacement: s.to_owned(),
            })
            .collect();

        Ok((word_start, candidates))
    }
}

// ── REPL entry point ──────────────────────────────────────────────────────────

pub async fn run_interactive(api: kw_clients::ApiClient) {
    println!(
        "{CYAN}kwctl interactive shell{RESET}  \
         {DIM}(type 'help' for commands, 'exit' to quit){RESET}"
    );

    let helper = ReplHelper {
        completer: SubcommandCompleter,
        hinter: HistoryHinter::new(),
        validator: MatchingBracketValidator::new(),
    };

    let config = rustyline::Config::builder()
        .history_ignore_space(true)
        .completion_type(rustyline::CompletionType::List)
        .build();

    let mut rl = match rustyline::Editor::with_config(config) {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("{RED}error:{RESET} failed to create editor: {e}");
            return;
        }
    };
    rl.set_helper(Some(helper));

    // Persist history across sessions
    let hist_path = directories::ProjectDirs::from("com", "", "knightwatch").map_or_else(
        || std::path::PathBuf::from(".kwctl_history"),
        |proj| proj.data_local_dir().join("cli").join("history"),
    );
    if let Some(parent) = hist_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = rl.load_history(&hist_path);

    loop {
        let readline = rl.readline("kwctl> ");
        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(trimmed);

                match trimmed {
                    "exit" | "quit" | "q" => {
                        println!("{DIM}Bye.{RESET}");
                        break;
                    }
                    "help" | "?" => {
                        print_help();
                        continue;
                    }
                    _ => {}
                }

                // Prepend a fake argv[0] so clap can parse the command
                let Some(args) = shlex::split(trimmed) else {
                    eprintln!("{RED}error:{RESET} could not tokenize input");
                    continue;
                };
                let args: Vec<String> = std::iter::once("kwctl".to_owned()).chain(args).collect();

                match crate::Cli::try_parse_from(&args) {
                    Ok(parsed) => {
                        if let Err(e) = crate::dispatch(parsed.command, &api).await {
                            eprintln!("{RED}error:{RESET} {e}");
                        }
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        let trimmed_msg = msg
                            .lines()
                            .take_while(|l| !l.starts_with("Usage:"))
                            .collect::<Vec<_>>()
                            .join("\n");
                        eprintln!("{trimmed_msg}");
                    }
                }
            }

            Err(ReadlineError::Interrupted) => {
                // Ctrl-C — clear the line, keep going
                println!("^C");
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D — exit cleanly
                println!("{DIM}Bye.{RESET}");
                break;
            }
            Err(e) => {
                eprintln!("{RED}readline error:{RESET} {e}");
                break;
            }
        }
    }

    let _ = rl.save_history(&hist_path);
}

fn print_help() {
    const SECTIONS: &[(&str, &[&str])] = &[
        ("common", &["health", "info", "shutdown"]),
        ("auth", &["login -u USER -p PASS", "logout"]),
        (
            "screen",
            &[
                "screenshot",
                "screen-poll-pause",
                "screen-poll-resume",
                "screen-poll-interval MS",
            ],
        ),
        (
            "process",
            &[
                "root-pids",
                "process-tree PID",
                "process-root PID",
                "process-children PID",
                "process-status PID",
                "process-is-done PID",
                "process-trees",
                "top-processes [--sort cpu|memory|disk] [--limit N]",
                "supported-signals",
                "kill-process PID [--signal SIGNAL]",
                "kill-tree PID",
                "track-pid PID",
                "untrack-pid PID",
                "process-poll-pause",
                "process-poll-resume",
                "process-poll-interval MS",
            ],
        ),
        (
            "resources",
            &[
                "system",
                "cpu",
                "memory",
                "disks",
                "networks",
                "gpus",
                "battery",
                "host-info",
                "temperatures",
                "alarms",
                "set-thresholds --cpu-warn F --memory-warn F --disk-warn F --battery-low F",
                "set-refresh-mask --cpu --memory --disks --networks --temperatures --gpus",
                "resources-poll-pause",
                "resources-poll-resume",
                "resources-poll-interval MS",
            ],
        ),
        (
            "systemd",
            &[
                "systemd",
                "unit NAME",
                "units-by-state STATE",
                "failed-units",
                "systemd-poll-pause",
                "systemd-poll-resume",
                "systemd-poll-interval MS",
            ],
        ),
        (
            "docker",
            &[
                "docker-containers",
                "container ID_OR_NAME",
                "top-containers [--sort cpu|memory] [--limit N]",
                "stop-container ID [--timeout-secs N]",
                "kill-container ID [--signal SIG]",
                "start-container ID",
                "restart-container ID [--timeout-secs N]",
                "pause-container ID",
                "unpause-container ID",
                "docker-poll-pause",
                "docker-poll-resume",
                "docker-poll-interval MS",
            ],
        ),
        ("meta", &["help", "exit / quit"]),
    ];

    println!();
    for (section, cmds) in SECTIONS {
        println!("  {YELLOW}{section}{RESET}");
        for cmd in *cmds {
            println!("    {CYAN}{cmd}{RESET}");
        }
        println!();
    }
}

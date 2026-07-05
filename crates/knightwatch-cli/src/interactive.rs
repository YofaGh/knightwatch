use clap::Parser;
use rustyline::{
    Completer, Helper, Hinter, Validator, completion::Pair, error::ReadlineError,
    hint::HistoryHinter, validate::MatchingBracketValidator,
};
use std::borrow::Cow;

use crate::colors::*;

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
        let word_start = line[..pos].rfind(' ').map(|i| i + 1).unwrap_or(0);
        let word = &line[word_start..pos];
        let has_space = line[..pos].contains(' ');

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

pub async fn run_interactive(api: crate::ApiClient) {
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

    let mut rl = rustyline::Editor::with_config(config).expect("failed to create editor");
    rl.set_helper(Some(helper));

    // Persist history across sessions
    let hist_path = directories::ProjectDirs::from("com", "", "knightwatch")
        .map(|proj| proj.data_local_dir().join("cli").join("history"))
        .unwrap_or_else(|| std::path::PathBuf::from(".kwctl_history"));
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
                let args = match shlex::split(trimmed) {
                    Some(a) => a,
                    None => {
                        eprintln!("{RED}error:{RESET} could not tokenize input");
                        continue;
                    }
                };
                let argv: Vec<String> = std::iter::once("kwctl".to_owned()).chain(args).collect();

                match crate::Cli::try_parse_from(&argv) {
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

//! spawn-agent spawns AI harnesses as teammates in tmux panes.

mod harness;
mod tmux;

use std::process;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};

/// Exit codes of the CLI contract. 0 is success.
const USAGE: i32 = 1;
const SCOPE: i32 = 2;
const ENVIRONMENT: i32 = 3;

/// A failed command: the message goes to stderr, the code to the shell.
#[derive(Debug)]
pub struct Fail {
    pub code: i32,
    pub message: String,
}

impl Fail {
    pub fn usage(message: impl Into<String>) -> Self {
        Fail {
            code: USAGE,
            message: message.into(),
        }
    }

    /// Unknown name, out-of-scope target and dead pane are one failure by design.
    pub fn scope(message: impl Into<String>) -> Self {
        Fail {
            code: SCOPE,
            message: message.into(),
        }
    }

    pub fn env(message: impl Into<String>) -> Self {
        Fail {
            code: ENVIRONMENT,
            message: message.into(),
        }
    }
}

#[derive(Parser)]
#[command(
    name = "spawn-agent",
    version,
    about = "Spawn AI harnesses as teammates in tmux panes"
)]
struct Cli {
    #[command(subcommand)]
    command: Verb,
}

#[derive(Subcommand)]
enum Verb {
    /// Spawn a teammate into a new pane of the current window.
    Spawn {
        /// Harness to run: claude or codex.
        #[arg(long, value_name = "NAME")]
        harness: String,
        /// Label for the teammate. The tool appends a unique suffix.
        #[arg(long, value_name = "BASE", default_value = "teammate")]
        name: String,
        /// Model, passed to the harness verbatim.
        #[arg(long, value_name = "M")]
        model: Option<String>,
        /// Reasoning effort, passed to the harness verbatim.
        #[arg(long, value_name = "E")]
        effort: Option<String>,
        /// Working directory of the teammate. Defaults to the caller's.
        #[arg(long, value_name = "DIR")]
        dir: Option<String>,
        /// The task for the teammate.
        #[arg(long, value_name = "TEXT")]
        prompt: Option<String>,
        /// Arguments passed to the harness command line untouched.
        #[arg(last = true, value_name = "HARNESS_ARGS")]
        harness_args: Vec<String>,
    },
    /// End a teammate you spawned.
    Kill {
        /// Name of the teammate.
        name: String,
    },
}

fn main() {
    let cli = Cli::try_parse().unwrap_or_else(|err| {
        let _ = err.print();
        // --help and --version print to stdout and are not failures.
        process::exit(if err.use_stderr() { USAGE } else { 0 });
    });
    if let Err(fail) = run(cli.command) {
        eprintln!("spawn-agent: {}", fail.message);
        process::exit(fail.code);
    }
}

fn run(verb: Verb) -> Result<(), Fail> {
    let caller = tmux::caller_pane()?;
    tmux::check_version()?;
    match verb {
        Verb::Spawn {
            harness,
            name,
            model,
            effort,
            dir,
            prompt,
            harness_args,
        } => {
            let argv = harness::command_line(&harness, model, effort, prompt, harness_args)?;
            spawn(&caller, &name, dir, argv)
        }
        Verb::Kill { name } => kill(&caller, &name),
    }
}

fn spawn(caller: &str, base: &str, dir: Option<String>, argv: Vec<String>) -> Result<(), Fail> {
    let taken: Vec<String> = tmux::panes()?.into_iter().map(|pane| pane.name).collect();
    let name = assign_name(base, &taken);

    // Target the caller's pane, not the session's active one, so the teammate joins the caller's
    // window. Without -c the new pane starts in this process's working directory.
    let mut split = vec!["split-window", "-t", caller, "-P", "-F", "#{pane_id}"];
    if let Some(dir) = &dir {
        split.push("-c");
        split.push(dir);
    }
    split.extend(argv.iter().map(String::as_str));
    let pane = tmux::run(&split)?;

    // Accepted cost: the harness starts before the stamps land, so a teammate that runs
    // spawn-agent in its first milliseconds resolves no lead.
    tmux::run(&["set-option", "-p", "-t", &pane, "@spawn_name", &name])?;
    tmux::run(&["set-option", "-p", "-t", &pane, "@spawn_lead", caller])?;
    tmux::run(&["select-layout", "-t", &pane, "main-vertical"])?;

    print!("{}", spawn_report(&name, &pane));
    Ok(())
}

/// A teammate is the caller's when its lead pointer is the caller's pane. An unknown name, another
/// lead's teammate and a pane that died just now all give the same refusal.
fn kill(caller: &str, name: &str) -> Result<(), Fail> {
    let refusal = || Fail::scope(format!("no teammate named {name}"));
    let panes = tmux::panes()?;
    let teammate = panes
        .iter()
        .find(|pane| pane.name == name && pane.lead == caller)
        .ok_or_else(refusal)?;
    tmux::run(&["kill-pane", "-t", &teammate.id]).map_err(|_| refusal())?;
    Ok(())
}

/// The stdout contract: the bare name first, then hint lines with the protocol for a lead that
/// has no other instructions.
fn spawn_report(name: &str, pane: &str) -> String {
    format!(
        "{name}\n\
         pane {pane}\n\
         send it instructions: spawn-agent send {name} \"...\"\n\
         read its reports: spawn-agent inbox\n"
    )
}

/// The caller's label plus a suffix. Names already stamped on panes are skipped, so uniqueness is
/// the tool's guarantee rather than the caller's.
fn assign_name(base: &str, taken: &[String]) -> String {
    std::iter::repeat_with(|| {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        format!("{base}-{}", name_suffix(elapsed, process::id()))
    })
    .find(|name| !taken.contains(name))
    .expect("the suffix source never ends")
}

/// Maildir-style: seconds, nanoseconds and pid, shortened to stay readable in a command line.
fn name_suffix(elapsed: Duration, pid: u32) -> String {
    format!(
        "{:x}{:04x}{:x}",
        elapsed.as_secs() % 0x10000,
        elapsed.subsec_nanos() % 0x10000,
        pid % 0x1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_across_rapid_spawns() {
        let mut taken: Vec<String> = Vec::new();
        for _ in 0..20 {
            let name = assign_name("teammate", &taken);
            assert!(name.starts_with("teammate-"), "{name} dropped the label");
            assert!(!taken.contains(&name), "{name} was already taken");
            taken.push(name);
        }
    }

    #[test]
    fn the_suffix_follows_the_clock_and_the_pid() {
        let one = name_suffix(Duration::new(17, 4), 1);
        assert_eq!(one, name_suffix(Duration::new(17, 4), 1));
        assert_ne!(one, name_suffix(Duration::new(17, 5), 1));
        assert_ne!(one, name_suffix(Duration::new(18, 4), 1));
        assert_ne!(one, name_suffix(Duration::new(17, 4), 2));
    }

    #[test]
    fn the_report_opens_with_the_bare_name() {
        let report = spawn_report("alpha-1f2", "%3");
        assert_eq!(report.lines().next(), Some("alpha-1f2"));
        assert!(report.contains("pane %3"));
        assert!(report.contains("spawn-agent send alpha-1f2"));
        assert!(report.contains("spawn-agent inbox"));
        assert!(report.ends_with('\n'));
    }
}

//! tmux is the registry. Teammate panes carry the `@spawn_name` and `@spawn_lead` pane user
//! options; every lookup queries tmux at call time. Nothing is cached and no roster file exists.

use std::env;
use std::process::Command;

use crate::Fail;

/// Pane user options require tmux 3.0.
const MIN_VERSION: (u32, u32) = (3, 0);

/// A pane stamped as a teammate.
pub struct Pane {
    pub id: String,
    pub name: String,
    /// Pane id of the lead that spawned this teammate.
    pub lead: String,
}

/// The calling pane's id. Both checks are environment failures.
pub fn caller_pane() -> Result<String, Fail> {
    if env::var_os("TMUX").is_none() {
        return Err(Fail::env(
            "spawn-agent runs inside tmux only. Start tmux, then run it again.",
        ));
    }
    env::var("TMUX_PANE").map_err(|_| {
        Fail::env("TMUX_PANE is not set, so spawn-agent cannot tell which pane called it.")
    })
}

pub fn check_version() -> Result<(), Fail> {
    let reported = run(&["-V"])?;
    match parse_version(&reported) {
        Some(version) if version < MIN_VERSION => Err(Fail::env(format!(
            "{reported} is too old. spawn-agent needs tmux {}.{} or newer.",
            MIN_VERSION.0, MIN_VERSION.1
        ))),
        _ => Ok(()),
    }
}

/// Every teammate pane on the server, in every session.
pub fn panes() -> Result<Vec<Pane>, Fail> {
    let listed = run(&[
        "list-panes",
        "-a",
        "-F",
        "#{pane_id}\t#{@spawn_name}\t#{@spawn_lead}",
    ])?;
    Ok(parse_panes(&listed))
}

/// Run a tmux command and return its trimmed stdout.
pub fn run(args: &[&str]) -> Result<String, Fail> {
    let output = Command::new("tmux")
        .args(args)
        .output()
        .map_err(|err| Fail::env(format!("cannot run tmux: {err}")))?;
    if !output.status.success() {
        return Err(Fail::env(format!(
            "tmux {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string())
}

/// "tmux 3.4a" gives (3, 4) and "tmux next-3.6" gives (3, 6). A build with no number, such as
/// "tmux master", gives None. Callers treat None as new enough.
pub fn parse_version(reported: &str) -> Option<(u32, u32)> {
    let number = reported
        .trim()
        .rsplit(' ')
        .next()?
        .trim_start_matches("next-");
    let (major, rest) = number.split_once('.')?;
    let minor: String = rest.chars().take_while(char::is_ascii_digit).collect();
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// A pane with no `@spawn_name` is not a teammate, so it is left out.
pub fn parse_panes(listed: &str) -> Vec<Pane> {
    listed
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let id = fields.next()?;
            let name = fields.next()?;
            let lead = fields.next()?;
            (!name.is_empty()).then(|| Pane {
                id: id.to_string(),
                name: name.to_string(),
                lead: lead.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("tmux 3.4"), Some((3, 4)));
        assert_eq!(parse_version("tmux 3.4a\n"), Some((3, 4)));
        assert_eq!(parse_version("tmux 2.9a"), Some((2, 9)));
        assert_eq!(parse_version("tmux next-3.6"), Some((3, 6)));
        assert_eq!(parse_version("tmux master"), None);
    }

    #[test]
    fn versions_below_the_floor_are_rejected() {
        assert!(parse_version("tmux 2.9a").unwrap() < MIN_VERSION);
        assert!(parse_version("tmux 3.0").unwrap() >= MIN_VERSION);
    }

    #[test]
    fn unstamped_panes_are_not_teammates() {
        let panes = parse_panes("%0\t\t\n%1\talpha-1f2\t%0\n%2\tbeta-3c4\t%1\n");
        let named: Vec<_> = panes.iter().map(|p| (&*p.id, &*p.name, &*p.lead)).collect();
        assert_eq!(named, [("%1", "alpha-1f2", "%0"), ("%2", "beta-3c4", "%1")]);
    }

    #[test]
    fn malformed_lines_are_skipped() {
        assert!(parse_panes("%0\n\n").is_empty());
    }
}

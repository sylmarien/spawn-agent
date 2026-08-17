//! Behaviour that needs a real tmux server, plus the environment checks that need none.
//! Each test owns a private tmux server and a stub harness on PATH.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

const BINARY: &str = env!("CARGO_BIN_EXE_spawn-agent");

/// A private tmux server with one session, and a stub `claude` on PATH so spawned panes survive.
struct Fixture {
    socket: String,
    directory: PathBuf,
    /// PATH with the stub harness in front.
    path: String,
    tmux_env: String,
    lead: String,
}

impl Fixture {
    fn new(label: &str) -> Fixture {
        let socket = format!("spawn-agent-test-{label}-{}", std::process::id());
        let directory = std::env::temp_dir().join(&socket);
        fs::create_dir_all(&directory).unwrap();
        let stub = directory.join("claude");
        fs::write(&stub, "#!/bin/sh\nexec sleep 300\n").unwrap();
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();

        let path = format!("{}:{}", directory.display(), std::env::var("PATH").unwrap());
        let started = Command::new("tmux")
            .args([
                "-L",
                &socket,
                "new-session",
                "-d",
                "-x",
                "80",
                "-y",
                "24",
                "sleep",
                "300",
            ])
            .env("PATH", &path)
            .env_remove("TMUX")
            .status()
            .expect("tmux is installed");
        assert!(started.success(), "cannot start the test tmux server");

        let mut fixture = Fixture {
            socket,
            directory,
            path,
            tmux_env: String::new(),
            lead: String::new(),
        };
        fixture.tmux_env = fixture.tmux(&["display-message", "-p", "#{socket_path},#{pid},0"]);
        fixture.lead = fixture.tmux(&["display-message", "-p", "#{pane_id}"]);
        fixture
    }

    /// Run a tmux command against the private server and return its trimmed stdout.
    fn tmux(&self, args: &[&str]) -> String {
        let output = Command::new("tmux")
            .args(["-L", &self.socket])
            .args(args)
            .env_remove("TMUX")
            .output()
            .unwrap();
        assert!(output.status.success(), "tmux {args:?} failed");
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string()
    }

    /// Run spawn-agent as if it were called from the given pane.
    fn call(&self, pane: &str, args: &[&str]) -> Output {
        Command::new(BINARY)
            .args(args)
            .env("TMUX", &self.tmux_env)
            .env("TMUX_PANE", pane)
            .env("PATH", &self.path)
            .output()
            .unwrap()
    }

    fn pane_ids(&self) -> Vec<String> {
        self.tmux(&["list-panes", "-a", "-F", "#{pane_id}"])
            .lines()
            .map(str::to_string)
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.socket, "kill-server"])
            .env_remove("TMUX")
            .status();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn spawn_stamps_the_pane_and_prints_the_contract() {
    let fixture = Fixture::new("spawn");
    let output = fixture.call(
        &fixture.lead,
        &[
            "spawn",
            "--harness",
            "claude",
            "--name",
            "alpha",
            "--model",
            "m1",
            "--dir",
            "/tmp",
            "--prompt",
            "do it",
            "--",
            "--verbose",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let printed = stdout(&output);
    let mut lines = printed.lines();
    let name = lines.next().unwrap();
    assert!(name.starts_with("alpha-"), "{name} is not a suffixed label");

    let teammate: Vec<String> = fixture
        .pane_ids()
        .into_iter()
        .filter(|id| *id != fixture.lead)
        .collect();
    assert_eq!(teammate.len(), 1, "spawn created exactly one pane");
    let pane = &teammate[0];

    assert_eq!(lines.next().unwrap(), format!("pane {pane}"));
    assert!(printed.contains(&format!("spawn-agent send {name} \"...\"")));
    assert!(printed.contains("spawn-agent inbox"));

    assert_eq!(
        fixture.tmux(&["show-options", "-p", "-t", pane, "-v", "@spawn_name"]),
        name
    );
    assert_eq!(
        fixture.tmux(&["show-options", "-p", "-t", pane, "-v", "@spawn_lead"]),
        fixture.lead
    );

    // --model, -- and --prompt reach the harness command line, in that order. tmux quotes the
    // argument holding a space, which shows the prompt stayed one argument.
    let command = fixture.tmux(&["display-message", "-p", "-t", pane, "#{pane_start_command}"]);
    assert_eq!(command, "claude --model m1 --verbose \"do it\"");

    // --dir sets the pane's working directory.
    assert_eq!(
        fixture.tmux(&["display-message", "-p", "-t", pane, "#{pane_current_path}"]),
        "/tmp"
    );
}

#[test]
fn kill_ends_only_the_callers_own_teammates() {
    let fixture = Fixture::new("kill");
    let spawned = fixture.call(
        &fixture.lead,
        &["spawn", "--harness", "claude", "--name", "beta"],
    );
    assert!(
        spawned.status.success(),
        "{}",
        String::from_utf8_lossy(&spawned.stderr)
    );
    let name = stdout(&spawned).lines().next().unwrap().to_string();
    let before = fixture.pane_ids().len();

    // Another lead sees the same refusal as for a name that does not exist.
    let stranger = fixture.call("%999", &["kill", &name]);
    let unknown = fixture.call(&fixture.lead, &["kill", "nobody-0000"]);
    assert_eq!(stranger.status.code(), Some(2));
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&stranger.stderr).replace(&name, "NAME"),
        String::from_utf8_lossy(&unknown.stderr).replace("nobody-0000", "NAME")
    );
    assert_eq!(
        fixture.pane_ids().len(),
        before,
        "a refused kill left every pane alone"
    );

    let killed = fixture.call(&fixture.lead, &["kill", &name]);
    assert!(
        killed.status.success(),
        "{}",
        String::from_utf8_lossy(&killed.stderr)
    );
    assert_eq!(fixture.pane_ids().len(), before - 1);
}

#[test]
fn every_verb_refuses_to_run_outside_tmux() {
    for args in [
        vec!["spawn", "--harness", "claude"],
        vec!["kill", "somebody"],
    ] {
        let output = Command::new(BINARY)
            .args(&args)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(3),
            "{args:?} did not report an environment failure"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("inside tmux"));
    }
}

#[test]
fn tmux_below_the_version_floor_is_refused() {
    let directory =
        std::env::temp_dir().join(format!("spawn-agent-test-old-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let stub = directory.join("tmux");
    fs::write(&stub, "#!/bin/sh\necho 'tmux 2.9a'\n").unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new(BINARY)
        .args(["kill", "somebody"])
        .env("TMUX", "/tmp/does-not-matter,1,0")
        .env("TMUX_PANE", "%0")
        .env("PATH", directory.display().to_string())
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&directory);

    assert_eq!(output.status.code(), Some(3));
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(complaint.contains("too old"), "{complaint}");
    assert!(complaint.contains("3.0"), "{complaint}");
}

#[test]
fn an_unknown_harness_is_a_usage_failure() {
    let fixture = Fixture::new("harness");
    let output = fixture.call(&fixture.lead, &["spawn", "--harness", "emacs"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown harness"));
}

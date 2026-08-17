//! Harness command lines. Bare launch: the pass-through values mapped onto each harness's flags.

use crate::Fail;

/// The argv the new pane runs. tmux executes it directly, so no shell quoting applies.
pub fn command_line(
    harness: &str,
    model: Option<String>,
    effort: Option<String>,
    prompt: Option<String>,
    extra: Vec<String>,
) -> Result<Vec<String>, Fail> {
    let mut argv = match harness {
        "claude" => {
            let mut argv = vec!["claude".to_string()];
            if let Some(model) = model {
                argv.push("--model".into());
                argv.push(model);
            }
            if let Some(effort) = effort {
                argv.push("--effort".into());
                argv.push(effort);
            }
            argv
        }
        "codex" => {
            let mut argv = vec!["codex".to_string()];
            if let Some(model) = model {
                argv.push("-m".into());
                argv.push(model);
            }
            if let Some(effort) = effort {
                // Codex parses `-c` values as TOML, so the string keeps its quotes.
                argv.push("-c".into());
                argv.push(format!("model_reasoning_effort=\"{effort}\""));
            }
            argv
        }
        other => {
            return Err(Fail::usage(format!(
                "unknown harness: {other}. Known harnesses: claude, codex."
            )));
        }
    };
    argv.extend(extra);
    argv.extend(prompt);
    Ok(argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(harness: &str, extra: Vec<String>, prompt: Option<&str>) -> Vec<String> {
        command_line(
            harness,
            Some("m1".into()),
            Some("high".into()),
            prompt.map(String::from),
            extra,
        )
        .unwrap()
    }

    #[test]
    fn claude_maps_model_and_effort() {
        assert_eq!(
            line("claude", vec![], Some("do it")),
            ["claude", "--model", "m1", "--effort", "high", "do it"]
        );
    }

    #[test]
    fn codex_maps_model_and_effort() {
        assert_eq!(
            line("codex", vec![], Some("do it")),
            [
                "codex",
                "-m",
                "m1",
                "-c",
                "model_reasoning_effort=\"high\"",
                "do it"
            ]
        );
    }

    #[test]
    fn extra_args_precede_the_prompt() {
        assert_eq!(
            line(
                "claude",
                vec!["--foo".into(), "bar baz".into()],
                Some("task")
            ),
            [
                "claude", "--model", "m1", "--effort", "high", "--foo", "bar baz", "task"
            ]
        );
    }

    #[test]
    fn optional_values_are_omitted() {
        assert_eq!(
            command_line("claude", None, None, None, vec![]).unwrap(),
            ["claude"]
        );
    }

    #[test]
    fn unknown_harness_is_a_usage_error() {
        assert_eq!(
            command_line("nope", None, None, None, vec![])
                .unwrap_err()
                .code,
            1
        );
    }
}

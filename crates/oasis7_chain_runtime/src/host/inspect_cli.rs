//! Offline inspection wrappers. These dispatch only existing bounded evidence
//! verifiers and have no Node, World startup, repair or deployment capability.
use std::collections::BTreeSet;

pub(super) fn run<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let command = match args.next() {
        None | Some("--help") => {
            if args.next().is_some() {
                return Err("inspect help accepts no other arguments".into());
            }
            print_help(None);
            return Ok(());
        }
        Some(command) => command,
    };
    let tail: Vec<&str> = args.collect();
    run_command(command, &tail)
}

pub(super) fn run_compatibility<'a>(
    command: &str,
    args: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    eprintln!("deprecated verifier spelling: use inspect {command}");
    let tail: Vec<&str> = args.collect();
    run_command(command, &tail)
}

fn run_command(command: &str, tail: &[&str]) -> Result<(), String> {
    let required: &[&str] = match command {
        "bootstrap" | "history" => &["--trusted-config", "--evidence"],
        "rebuild-proof" => &[
            "--proof",
            "--trusted-signer-id",
            "--trusted-signer-public-key-hex",
        ],
        _ => return Err(format!("unknown inspect command: {command}")),
    };
    if tail.is_empty() || tail == ["--help"] || tail == ["-h"] {
        print_help(Some(command));
        return Ok(());
    }
    let mut seen = BTreeSet::new();
    for pair in tail.chunks(2) {
        let flag = pair[0];
        if !required.contains(&flag) {
            return Err(format!("unknown inspect {command} option: {flag}"));
        }
        if !seen.insert(flag) {
            return Err(format!("duplicate inspect {command} option: {flag}"));
        }
        let value = pair
            .get(1)
            .filter(|value| !value.trim().is_empty() && !value.starts_with("--"))
            .ok_or_else(|| format!("inspect {command} {flag} requires a value"))?;
        let _ = value;
    }
    for flag in required {
        if !seen.contains(flag) {
            return Err(format!("inspect {command} requires {flag}"));
        }
    }
    match command {
        "bootstrap" => super::controlled_bootstrap_cli::run(tail.iter().copied()),
        "history" => super::controlled_history_cli::run(tail.iter().copied()),
        "rebuild-proof" => super::rebuild_status::run_verify(tail.iter().copied()),
        _ => unreachable!("validated inspect command"),
    }
}
fn print_help(command: Option<&str>) {
    let usage = match command {
        Some("bootstrap") => "inspect bootstrap --trusted-config PATH --evidence PATH",
        Some("history") => "inspect history --trusted-config PATH --evidence PATH",
        Some("rebuild-proof") => {
            "inspect rebuild-proof --proof PATH --trusted-signer-id ID --trusted-signer-public-key-hex HEX"
        }
        _ => "inspect bootstrap|history|rebuild-proof --help",
    };
    println!(
        "{usage}\nOffline existing-evidence verification only. No World/Node startup, repair, state changes or readiness grant. Old verify-controlled-bootstrap/history/verify-rebuild-proof spellings are compatibility aliases."
    );
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn inspect_help_and_invalid_input_have_no_local_node_effects() {
        let absent = std::env::temp_dir().join(format!(
            "oasis7-inspect-no-effects-{}",
            crate::node_lifecycle::local_io::random_id().unwrap()
        ));
        assert!(!absent.exists());
        for command in ["bootstrap", "history", "rebuild-proof"] {
            assert!(
                crate::identity_receipt::dispatch(["inspect", command, "--help"].into_iter())
                    .unwrap()
                    .is_ok()
            );
        }
        let path = absent.to_str().unwrap();
        assert!(
            run([
                "history",
                "--trusted-config",
                path,
                "--trusted-config",
                path
            ]
            .into_iter())
            .unwrap_err()
            .contains("duplicate")
        );
        assert!(
            run(["bootstrap", "--trusted-config", "--evidence"].into_iter())
                .unwrap_err()
                .contains("requires a value")
        );
        assert!(
            run(["rebuild-proof", "--proof", path, "--proof", path].into_iter())
                .unwrap_err()
                .contains("duplicate")
        );
        assert!(run(["repair", "--config", path].into_iter()).is_err());
        assert!(!absent.exists());
    }
}

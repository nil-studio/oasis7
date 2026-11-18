//! Process adapter for the reusable chain runtime library.

fn main() -> std::process::ExitCode {
    oasis7::observability::init_tracing("oasis7_chain_runtime");
    match oasis7_chain_runtime::run_cli(std::env::args().skip(1).collect()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            report_public_failure(&error, &mut std::io::stderr().lock());
            std::process::ExitCode::FAILURE
        }
    }
}

// These exact public diagnostics must survive failed stdout/tracing delivery.
// Never match prefixes or mirror arbitrary errors: they can contain private data.
const PUBLIC_DELIVERY_FAILURE: &str =
    "public header Node report delivery failed; actions may already be committed; do not resubmit";

const PUBLIC_NODE_FAILURES: &[&str] = &[
    PUBLIC_DELIVERY_FAILURE,
    "header Node actual qualified H driver recovery rejected",
    "header Node actual qualified H driver recovery rejected; recovery_stage=storage_paths",
    "header Node actual qualified H driver recovery rejected; recovery_stage=private_store",
    "header Node actual qualified H driver recovery rejected; recovery_stage=ledger",
    "header Node actual qualified H driver recovery rejected; recovery_stage=ledger_handoff",
    "header Node actual qualified H driver recovery rejected; recovery_stage=ledger_open",
    "header Node actual qualified H driver recovery rejected; recovery_stage=ledger_preload",
    "header Node actual qualified H driver recovery rejected; recovery_stage=ledger_replay",
    "header Node actual qualified H driver recovery rejected; recovery_stage=ledger_origin",
    "header Node actual qualified H driver recovery rejected; recovery_stage=h_index",
    "header Node actual qualified H driver recovery rejected; recovery_stage=bind_origin",
    "header Node actual qualified H driver recovery rejected; recovery_stage=unknown",
    "header Node actual startup recovery rejected",
    "header Node canonical private actions rejected",
    "header Node private action admission rejected",
    "header Node blocked before complete applied receipt",
    "header Node timed out before complete applied receipt",
    "header Node interrupted before complete applied receipt",
    "header Node could not stop cleanly",
    "header Node identity rejected",
    "header Node replication storage rejected",
    "header Node runtime configuration rejected",
    "header Node requester key rejected",
    "header Node qualified H trust rejected",
    "header Node qualified H material rejected",
    "header Node trusted signer binding rejected",
    "header Node explicit remote signer trust rejected",
    "header Node binding digest rejected",
    "header Node state directory resolution rejected",
    "invalid header Node config",
];

fn report_public_failure(error: &str, stderr: &mut impl std::io::Write) {
    if let Some(public) = PUBLIC_NODE_FAILURES.iter().find(|public| **public == error) {
        // Emit the static allowlist value, never the received error. A second
        // output failure must not panic or change the already-failed exit status.
        let _ = writeln!(stderr, "{public}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_stage_diagnostics_survive_failed_stdout() {
        let error =
            "header Node actual qualified H driver recovery rejected; recovery_stage=private_store";
        let mut output = Vec::new();
        report_public_failure(error, &mut output);
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "header Node actual qualified H driver recovery rejected; recovery_stage=private_store\n"
        );
        for error in [
            "header Node actual qualified H driver recovery rejected; recovery_stage=foreign",
            "header Node actual qualified H driver recovery rejected; recovery_stage=ledger: private/path/token",
        ] {
            let mut output = Vec::new();
            report_public_failure(error, &mut output);
            assert!(output.is_empty());
        }
    }

    #[test]
    fn every_allowlisted_error_is_exact_and_bounded() {
        for public in PUBLIC_NODE_FAILURES {
            let mut output = Vec::new();
            report_public_failure(public, &mut output);
            assert_eq!(output, format!("{public}\n").as_bytes());
            assert!(output.len() < 128);
        }
    }

    #[test]
    fn private_unknown_and_near_match_errors_are_not_echoed() {
        let mut output = Vec::new();
        for public in PUBLIC_NODE_FAILURES {
            for error in [
                format!("{public}: /private/key token=secret"),
                format!("/private/key: {public}"),
                format!("{public}\nsecret"),
                format!("{public}\0secret"),
                format!(" {public}"),
                format!("{public} "),
                public.to_uppercase(),
            ] {
                report_public_failure(&error, &mut output);
            }
        }
        for error in ["", "private input failure", "/private/key token=secret"] {
            report_public_failure(error, &mut output);
        }
        report_public_failure(&"x".repeat(1024 * 1024), &mut output);
        assert!(output.is_empty());
    }

    #[test]
    fn failed_stderr_does_not_panic() {
        struct Failed;
        impl std::io::Write for Failed {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        for public in PUBLIC_NODE_FAILURES {
            report_public_failure(public, &mut Failed);
        }
    }
}

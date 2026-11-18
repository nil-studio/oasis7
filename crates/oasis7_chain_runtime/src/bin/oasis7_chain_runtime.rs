//! Process adapter for the reusable chain runtime library.

fn main() -> std::process::ExitCode {
    oasis7::observability::init_tracing("oasis7_chain_runtime");
    match oasis7_chain_runtime::run_cli(std::env::args().skip(1).collect()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            report_delivery_failure(&error, &mut std::io::stderr().lock());
            std::process::ExitCode::FAILURE
        }
    }
}

// This public, fixed diagnostic must survive a failed stdout delivery. Do not
// mirror arbitrary library errors: those can contain private paths or inputs.
const PUBLIC_DELIVERY_FAILURE: &str =
    "public header Node report delivery failed; actions may already be committed; do not resubmit";

fn report_delivery_failure(error: &str, stderr: &mut impl std::io::Write) {
    if error == PUBLIC_DELIVERY_FAILURE {
        // A second output failure must not panic or change the failed exit status.
        let _ = writeln!(stderr, "{PUBLIC_DELIVERY_FAILURE}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_warning_is_exact_and_does_not_echo_private_errors() {
        let mut output = Vec::new();
        report_delivery_failure(PUBLIC_DELIVERY_FAILURE, &mut output);
        assert_eq!(output, format!("{PUBLIC_DELIVERY_FAILURE}\n").as_bytes());
        output.clear();
        report_delivery_failure("private input failure", &mut output);
        report_delivery_failure(
            &format!("{PUBLIC_DELIVERY_FAILURE}: private path"),
            &mut output,
        );
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
        report_delivery_failure(PUBLIC_DELIVERY_FAILURE, &mut Failed);
    }
}

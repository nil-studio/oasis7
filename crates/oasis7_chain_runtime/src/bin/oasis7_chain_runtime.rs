//! Process adapter for the reusable chain runtime library.

fn main() -> std::process::ExitCode {
    oasis7::observability::init_tracing("oasis7_chain_runtime");
    match oasis7_chain_runtime::run_cli(std::env::args().skip(1).collect()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::FAILURE,
    }
}

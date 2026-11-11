//! Process adapter for the reusable chain runtime library.

fn main() -> std::process::ExitCode {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| {
            arg == "--oasis7-native-bound-worker-v1"
                || arg == "--oasis7-native-bound-regex-worker-v1"
        })
    {
        return match oasis7_chain_runtime::native_bound_worker_main_v1() {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(_) => std::process::ExitCode::FAILURE,
        };
    }

    if std::env::args_os().nth(1).is_some_and(|arg| {
        arg == "--oasis7-wasm-worker-v1" || arg == "--oasis7-wasm-worker-handshake-v1"
    }) {
        return match oasis7_wasm_executor::bounded_worker::worker_main_v1() {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(_) => std::process::ExitCode::FAILURE,
        };
    }
    oasis7::observability::init_tracing("oasis7_chain_runtime");
    match oasis7_chain_runtime::run_cli(std::env::args().skip(1).collect()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::FAILURE,
    }
}

//! Dedicated test-support image: descriptor capture and invocation use this ELF.
fn main() -> std::process::ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = match args.first().and_then(|argument| argument.to_str()) {
        Some("--capture-native-fixture-v1") if args.len() <= 2 => {
            oasis7_chain_runtime::native_worker_fixture_capture_v1(
                args.get(1).map(std::path::Path::new),
            )
            .and_then(|bytes| {
                use std::io::Write;
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|error| error.to_string())
            })
        }
        Some("--oasis7-native-bound-worker-v1" | "--oasis7-native-bound-regex-worker-v1") => {
            oasis7_chain_runtime::native_worker_fixture_main_v1().map_err(str::to_owned)
        }
        Some("--oasis7-wasm-worker-v1" | "--oasis7-wasm-worker-handshake-v1") => {
            oasis7_wasm_executor::bounded_worker::worker_main_v1().map_err(str::to_owned)
        }
        _ => Err("native fixture mode required".into()),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

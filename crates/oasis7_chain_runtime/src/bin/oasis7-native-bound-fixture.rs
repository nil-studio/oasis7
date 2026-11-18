//! Dedicated test-support image: descriptor capture and invocation use this ELF.
fn main() -> std::process::ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = match args.first().and_then(|argument| argument.to_str()) {
        #[cfg(all(unix, feature = "iws-host-native-test-support"))]
        Some(mode @ ("--iws-normal-return-work-v1" | "--iws-normal-partial-source-v1"))
            if args.len() == 2 =>
        {
            oasis7_chain_runtime::native_worker_fixture_iws_source_work_return_v1(
                std::path::Path::new(&args[1]),
                mode == "--iws-normal-partial-source-v1",
            )
            .and_then(|bytes| {
                use std::io::Write;
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|error| error.to_string())
            })
        }
        #[cfg(all(unix, feature = "iws-host-native-test-support"))]
        Some("--iws-checkpoint-suffix-v1") if args.len() == 2 => {
            oasis7_chain_runtime::native_worker_fixture_iws_checkpoint_suffix_v1(
                std::path::Path::new(&args[1]),
            )
            .and_then(|bytes| {
                use std::io::Write;
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|error| error.to_string())
            })
        }
        #[cfg(all(unix, feature = "iws-host-native-test-support"))]
        Some(mode @ ("--iws-normal-cold-before-v1" | "--iws-normal-cold-after-v1"))
            if args.len() == 2 =>
        {
            oasis7_chain_runtime::native_worker_fixture_iws_normal_cold_failure_v1(
                std::path::Path::new(&args[1]),
                mode == "--iws-normal-cold-before-v1",
            )
            .and_then(|bytes| {
                use std::io::Write;
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|error| error.to_string())
            })
        }
        #[cfg(all(unix, feature = "iws-host-native-test-support"))]
        Some("--iws-nonempty-normal-cold-v1") if args.len() == 2 => {
            oasis7_chain_runtime::native_worker_fixture_iws_nonempty_normal_cold_v1(
                std::path::Path::new(&args[1]),
            )
            .and_then(|bytes| {
                use std::io::Write;
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|error| error.to_string())
            })
        }
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

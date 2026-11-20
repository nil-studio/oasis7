fn main() {
    if let Err(err) = wasm_build_suite::cli::run_cli() {
        eprintln!("error: {err}");
        std::process::exit(2);
    }
}

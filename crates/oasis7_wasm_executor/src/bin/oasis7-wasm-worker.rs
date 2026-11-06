//! Installed host worker, not a guest-selected program. Errors are intentionally
//! fixed and silent; structured VM failure codes use the bounded output frame.
fn main() {
    if oasis7_wasm_executor::bounded_worker::worker_main_v1().is_err() {
        std::process::exit(1);
    }
}

fn main() {
    #[cfg(unix)]
    if oasis7_game_client::dev_test_recovery::run(std::env::args().skip(1).collect()).is_err() {
        eprintln!("operator_recovery_failed");
        std::process::exit(1);
    }
    #[cfg(not(unix))]
    {
        eprintln!("unix_required");
        std::process::exit(1);
    }
}

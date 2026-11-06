fn main() {
    #[cfg(unix)]
    if oasis7_game_client::dev_test_sessions::run(std::env::args().skip(1).collect()).is_err() {
        eprintln!("read_session_service_failed");
        std::process::exit(1)
    }
    #[cfg(not(unix))]
    {
        eprintln!("unix_required");
        std::process::exit(1)
    }
}

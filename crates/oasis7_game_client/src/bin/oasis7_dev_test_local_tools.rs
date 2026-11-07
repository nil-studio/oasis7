fn main() {
    #[cfg(unix)]
    if oasis7_game_client::dev_test_local_business::run(std::env::args().skip(1).collect()).is_err()
    {
        eprintln!("local_tools_failed");
        std::process::exit(1)
    }
    #[cfg(not(unix))]
    {
        eprintln!("linux_required");
        std::process::exit(1)
    }
}

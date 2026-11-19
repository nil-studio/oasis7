//! Explicit profiles for the existing prepared-world live-service runner.
use std::{path::PathBuf, time::Duration};

pub(super) fn run(ready: PathBuf) {
    let profile = match std::env::var("OASIS7_TASK_STARTER_CHAIN_PLAYTEST_PROFILE") {
        Ok(profile) => profile,
        Err(std::env::VarError::NotPresent) => "claimed_agent".into(),
        Err(error) => panic!("invalid starter-chain playtest profile: {error}"),
    };
    match profile.as_str() {
        // Preserve the original explicit fixture, including its cadence and
        // cleanup contract, when no alternative profile was requested.
        "claimed_agent" => {
            super::super::run_prepared_world_service_kind(
                ready,
                Duration::from_secs(5),
                false,
                None,
                false,
                Duration::from_secs(30),
                super::super::PreparedWorldKind::StarterChain,
                Duration::ZERO,
            );
        }
        // This creates a new participant through the public service and its
        // finite public pool. No claimed-Agent fixture or grants are reused.
        "ordinary_first_session" => {
            super::super::super::participant_http::run_ordinary_starter_prepared_world_service(
                &ready,
            );
        }
        _ => panic!("unknown starter-chain playtest profile: {profile}"),
    }
}

//! Separate trusted daemon and model-facing socket client entry points.
use std::{
    io::{IsTerminal, Read},
    path::Path,
};

pub(crate) const HELP: &str = "broker-serve --profile <private-profile.json> | broker-profile --profile <private-profile.json> | broker-call --socket <approved-socket>\nServe reads private unlock from trusted stdin. Call reads a bounded BrokerRequest JSON from stdin and never accepts unlock, owner commands or config paths. A profile does not establish Runtime sandbox isolation or cold-entry acceptance.";

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 3 {
        return Err(HELP.into());
    }
    match (args[0].as_str(), args[1].as_str()) {
        ("broker-profile", "--profile") => {
            let config = crate::runtime_broker::BrokerConfig::load(Path::new(&args[2]))?;
            crate::device_client::print(
                &serde_json::json!({"state":"profile_validated","runtime_id":config.runtime_id,"source_revision":config.source_revision,"complete_cold_entry":false}),
            )
        }
        ("broker-serve", "--profile") => {
            if std::io::stdin().is_terminal() {
                return Err("action_required: trusted private unlock stdin required".into());
            }
            let mut unlock = zeroize::Zeroizing::new(Vec::new());
            std::io::stdin()
                .take(1025)
                .read_to_end(&mut unlock)
                .map_err(|_| "private unlock unavailable")?;
            if !(12..=1024).contains(&unlock.len()) {
                return Err("private unlock requires 12 to 1024 bytes".into());
            }
            crate::runtime_broker::serve(Path::new(&args[2]), unlock)
        }
        ("broker-call", "--socket") => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .take(32769)
                .read_to_end(&mut bytes)
                .map_err(|_| "tool request unavailable")?;
            if bytes.len() > 32768 {
                return Err("tool request exceeds bound".into());
            }
            let request = oasis7_agent_api::game::parse_request(&bytes)
                .map_err(|_| "invalid broker request")?;
            crate::device_client::print(&crate::runtime_broker::call(
                Path::new(&args[2]),
                &request,
            )?)
        }
        _ => Err(HELP.into()),
    }
}

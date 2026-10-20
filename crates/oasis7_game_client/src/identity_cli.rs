//! Native identity owner-control entry. Unlock bytes never come from argv/env.
use crate::identity_client::{IdentityClient, IdentityClientConfig};
use serde_json::json;
use std::io::{IsTerminal, Read};
use zeroize::Zeroizing;

pub(crate) const HELP: &str = "oasis7_game_client --identity-config <identity.json> identity-init|identity-public-key|identity-register|identity-login|identity-status [--operation <original-id>] [--name <display-name>]\nUnlock passphrase must be provided on private stdin by the trusted local user/launcher, never in argv, environment, chat or model tools. This owner-control entry does not start a runtime or enter a world.";
pub(crate) fn run(args: Vec<String>) -> Result<(), String> {
    if args.len() < 3 {
        return Err(HELP.into());
    }
    let command = args[2].as_str();
    let allowed: &[&str] = match command {
        "identity-init" | "identity-public-key" => &[],
        "identity-login" | "identity-status" => &["--operation"],
        "identity-register" => &["--operation", "--name"],
        _ => return Err(HELP.into()),
    };
    if !(args.len() - 3).is_multiple_of(2) {
        return Err(HELP.into());
    }
    let mut options = std::collections::BTreeMap::new();
    for pair in args[3..].chunks_exact(2) {
        if !allowed.contains(&pair[0].as_str())
            || options.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err("unsupported or duplicate identity option".into());
        }
    }
    if options.len() != allowed.len() {
        return Err(HELP.into());
    }
    let file = std::fs::File::open(&args[1]).map_err(|_| "identity configuration unavailable")?;
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "identity configuration unavailable")?;
    if bytes.len() > 65536 {
        return Err("identity configuration exceeds byte limit".into());
    }
    let value = oasis7_agent_api::game::parse_request(&bytes)
        .map_err(|_| "invalid identity configuration")?;
    let config: IdentityClientConfig =
        serde_json::from_value(value).map_err(|_| "invalid identity configuration")?;
    if std::io::stdin().is_terminal() {
        return Err("action_required: unlock through trusted local private stdin; do not put secrets in model context".into());
    }
    let mut passphrase = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(1025)
        .read_to_end(&mut passphrase)
        .map_err(|_| "identity unlock input unavailable")?;
    if !(12..=1024).contains(&passphrase.len()) {
        return Err("identity unlock requires 12 to 1024 private bytes".into());
    }
    if command == "identity-init" {
        let public = IdentityClient::initialize(&config, &passphrase)?;
        return crate::device_client::print(
            &json!({"state":"initialized","public_key":public,"network_effects":false}),
        );
    }
    let client = IdentityClient::new(config, &passphrase)?;
    match command {
        "identity-public-key" => {
            crate::device_client::print(&json!({"public_key":client.public_key()}))
        }
        "identity-register" => crate::device_client::print(
            &client.register(options["--operation"], options["--name"])?,
        ),
        "identity-login" => crate::device_client::print(&client.login(options["--operation"])?),
        "identity-status" => crate::device_client::print(&client.status(options["--operation"])?),
        _ => unreachable!(),
    }
}

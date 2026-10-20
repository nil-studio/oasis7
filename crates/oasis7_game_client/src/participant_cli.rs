use crate::identity_client::{ParticipantClient, ParticipantClientConfig};
use std::io::{IsTerminal, Read};
use zeroize::Zeroizing;
pub(crate) const HELP: &str = "oasis7_game_client --entry-config <entry.json> entry-owner-init|entry-register|entry-owner-view|entry-status|entry-task-create|entry-starter-claim|entry-starter-eligibility|entry-goal-prepare|entry-goal-publish|entry-goal-read|entry-executor-authorize|entry-executor-bind|entry-executor-stop|entry-executor-revoke|entry-executor-init|entry-owner-status [--operation <original-id>] [--login-operation <identity-login-id>]\nTrusted local owner control only; unlock through private stdin. Owner operations require explicit commands. No command starts a Runtime or proves complete cold entry. Goal content is read from --goal-file as JSON and never printed. Executor credentials use a separate --executor-directory.\nOwner writes require --participant-operation <entry-op> --login-operation <login-op> --operation <original-op>.\nGoal prepare adds --goal-file <private-content.json>; publication adds --prepare-operation <original-prepare-op>.\nExecutor authorize/revoke adds --executor-public-key <key> --generation <u64> --nonce <u64>; bind adds --executor-public-key <key> --generation <u64> --executor-id <id>.\nOwner status requires --participant-operation <entry-op> --operation <original-op>; goal read requires --participant-operation <entry-op> --login-operation <login-op>.";
pub(crate) fn run(args: Vec<String>) -> Result<(), String> {
    if args.len() < 3 {
        return Err(HELP.into());
    }
    let allowed: &[&str] = match args[2].as_str() {
        "entry-owner-init" => &[],
        "entry-register" | "entry-owner-view" => &["--operation", "--login-operation"],
        "entry-status" => &["--operation"],
        "entry-task-create" | "entry-starter-claim" => &[
            "--participant-operation",
            "--login-operation",
            "--operation",
        ],
        "entry-owner-status" => &["--participant-operation", "--operation"],
        "entry-goal-prepare" => &[
            "--participant-operation",
            "--login-operation",
            "--operation",
            "--goal-file",
        ],
        "entry-goal-publish" => &[
            "--participant-operation",
            "--login-operation",
            "--operation",
            "--prepare-operation",
        ],
        "entry-goal-read" | "entry-starter-eligibility" => {
            &["--participant-operation", "--login-operation"]
        }
        "entry-executor-init" => &["--executor-directory"],
        "entry-executor-authorize" | "entry-executor-revoke" => &[
            "--participant-operation",
            "--login-operation",
            "--operation",
            "--executor-public-key",
            "--generation",
            "--nonce",
        ],
        "entry-executor-stop" => &[
            "--participant-operation",
            "--login-operation",
            "--operation",
        ],
        "entry-executor-bind" => &[
            "--participant-operation",
            "--login-operation",
            "--operation",
            "--executor-public-key",
            "--generation",
            "--executor-id",
        ],
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
            return Err(HELP.into());
        }
    }
    if options.len() != allowed.len() {
        return Err(HELP.into());
    }
    let file = std::fs::File::open(&args[1]).map_err(|_| "entry config unavailable")?;
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "entry config unavailable")?;
    if bytes.len() > 65536 {
        return Err("entry config exceeds byte bound".into());
    }
    let config: ParticipantClientConfig =
        oasis7_agent_api::game::parse_request(&bytes).map_err(|_| "invalid entry config")?;
    if std::io::stdin().is_terminal() {
        return Err("action_required: trusted local private stdin required; never place owner unlock in model context".into());
    }
    let mut password = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(1025)
        .read_to_end(&mut password)
        .map_err(|_| "private unlock unavailable")?;
    if !(12..=1024).contains(&password.len()) {
        return Err("private unlock requires 12 to 1024 bytes".into());
    }
    if args[2] == "entry-owner-init" {
        return crate::device_client::print(
            &serde_json::json!({"state":"owner_initialized","owner_public_key":ParticipantClient::initialize_owner(&config,&password)?,"network_effects":false}),
        );
    }
    let client = ParticipantClient::new(config, &password)?;
    if args[2] == "entry-owner-view" {
        return crate::device_client::print(
            &client.current_owner(options["--operation"], options["--login-operation"])?,
        );
    }
    if let Some(result) = run_owner_command(&client, &args[2], &options, &password)? {
        return crate::device_client::print(&result);
    }
    let result = match args[2].as_str() {
        "entry-register" => {
            client.register(options["--operation"], options["--login-operation"])?
        }
        "entry-status" => client.status(options["--operation"])?,
        _ => unreachable!(),
    };
    crate::device_client::print(&result)
}

fn run_owner_command(
    client: &ParticipantClient,
    command: &str,
    options: &std::collections::BTreeMap<&str, &str>,
    password: &[u8],
) -> Result<Option<serde_json::Value>, String> {
    let encode = |value| Some(value);
    let value=match command {
        "entry-owner-status"=>serde_json::to_value(client.owner_operation_status(options["--participant-operation"],options["--operation"])?),
        "entry-task-create"=>serde_json::to_value(client.create_task(options["--participant-operation"],options["--login-operation"],options["--operation"])?),
        "entry-starter-claim"=>serde_json::to_value(client.claim_starter(options["--participant-operation"],options["--login-operation"],options["--operation"])?),
        "entry-starter-eligibility"=>Ok(client.starter_eligibility(options["--participant-operation"],options["--login-operation"])?),
        "entry-goal-prepare"=>{
            let file=std::fs::File::open(options["--goal-file"]).map_err(|_|"private goal input unavailable")?;
            let mut bytes=zeroize::Zeroizing::new(Vec::new());file.take(4097).read_to_end(&mut bytes).map_err(|_|"private goal input unavailable")?;
            if bytes.len()>4096 {return Err("complete private goal exceeds bound".into());}
            let content=oasis7_agent_api::game::parse_request(&bytes).map_err(|_|"invalid complete private goal")?;
            let receipt=client.prepare_goal(options["--participant-operation"],options["--login-operation"],options["--operation"],content)?;
            Ok(serde_json::json!({"operation_id":options["--operation"],"stage":"goal_prepare","state":"confirmed","goal_ref":receipt.goal_ref,"next_action":"explicit_owner_publication_required"}))
        },
        "entry-goal-publish"=>serde_json::to_value(client.publish_prepared_goal(options["--participant-operation"],options["--login-operation"],options["--operation"],options["--prepare-operation"])?),
        "entry-goal-read"=>{let current=client.read_current_goal(options["--participant-operation"],options["--login-operation"])?;Ok(serde_json::json!({"stage":"current_goal","state":"confirmed","control":current.control,"read_commit":current.read_commit,"next_action":"explicit_executor_authorization_required"}))},
        "entry-executor-init"=>Ok(serde_json::json!({"stage":"executor_vault","state":"confirmed","executor_public_key":client.initialize_executor(std::path::Path::new(options["--executor-directory"]),password)?,"network_effects":false,"next_action":"explicit_owner_delegation_required"})),
        "entry-executor-authorize"=>serde_json::to_value(client.authorize_executor(options["--participant-operation"],options["--login-operation"],options["--operation"],options["--executor-public-key"],decimal_option(options,"--generation")?,decimal_option(options,"--nonce")?)?),
        "entry-executor-stop"=>serde_json::to_value(client.stop_executor(options["--participant-operation"],options["--login-operation"],options["--operation"])?),
        "entry-executor-revoke"=>serde_json::to_value(client.revoke_executor(options["--participant-operation"],options["--login-operation"],options["--operation"],options["--executor-public-key"],decimal_option(options,"--generation")?,decimal_option(options,"--nonce")?)?),
        "entry-executor-bind"=>serde_json::to_value(client.bind_executor(options["--participant-operation"],options["--login-operation"],options["--operation"],options["--executor-id"],options["--executor-public-key"],decimal_option(options,"--generation")?)?),
        _=>return Ok(None),
    }.map_err(|_|"owner progress encoding failed")?;
    Ok(encode(value))
}
fn decimal_option(
    options: &std::collections::BTreeMap<&str, &str>,
    name: &str,
) -> Result<u64, String> {
    let value = options[name];
    let n = value
        .parse::<u64>()
        .map_err(|_| "owner numeric option requires canonical u64")?;
    if value != n.to_string() {
        return Err("owner numeric option requires canonical u64".into());
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starter_commands_dispatch_to_sdk_instead_of_falling_through() {
        const PASSWORD: &[u8] = b"synthetic owner CLI dispatcher unlock";
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).unwrap();
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("owner-cli-dispatch-{}", hex::encode(random)));
        std::fs::create_dir(&root).unwrap();
        let identity = crate::identity_client::IdentityClientConfig {
            endpoint: "https://identity.invalid".into(),
            identity_service_id: "identity".into(),
            environment_id: "private".into(),
            audience: "https://identity.invalid".into(),
            state_directory: root.join("identity"),
            test_loopback: false,
        };
        let config = ParticipantClientConfig {
            identity,
            identity_instance_epoch: oasis7_agent_api::game::DecimalU64(1),
            world: oasis7_client_api::world_service::WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
            },
            trusted_world_service_key: oasis7_client_api::world_service::sign_read_request(
                "fixture",
                (),
                &hex::encode([95; 32]),
            )
            .unwrap()
            .subject_public_key,
            owner_state_directory: root.join("owner"),
        };
        crate::identity_client::IdentityClient::initialize(&config.identity, PASSWORD).unwrap();
        ParticipantClient::initialize_owner(&config, PASSWORD).unwrap();
        let client = ParticipantClient::new(config, PASSWORD).unwrap();
        let options = std::collections::BTreeMap::from([
            ("--participant-operation", "missing-original"),
            ("--login-operation", "missing-login"),
            ("--operation", "starter-original"),
        ]);
        // No participant original exists, so each recognized SDK call fails
        // before networking. Ok(None) means the command was silently dropped.
        for command in ["entry-starter-claim", "entry-starter-eligibility"] {
            assert!(run_owner_command(&client, command, &options, PASSWORD).is_err());
        }
        drop(client);
        std::fs::remove_dir_all(root).unwrap();
    }
}

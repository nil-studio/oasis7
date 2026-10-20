//! Read-only authenticated service diagnostics; dispatch before node startup.
use oasis7::world_service::{
    CommitRef, MembershipCandidateStatus, ReadMembershipRequest, ReadMembershipResponse,
    WORLD_SERVICE_CONTRACT_VERSION, authority,
    client::{RemoteWorldServiceClient, WorldServiceClientConfig},
    verified_view::ReadEvidencePolicy,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
#[path = "membership_cursor_io.rs"]
mod cursor_io;

const HELP: &str = "Usage: oasis7_chain_runtime validator <list|status|wait|watch> [--node-id <id>] [--state effective|revoked|registered|approved] [--timeout-ms <1..3600000>] [--interval-ms <100..10000>] [--cursor <CommitRef JSON> | --cursor-file <absolute-public-proof-file>] [--cursor-out <absolute-public-proof-file>] [--allow-controlled-prerequisite]\nUses OASIS7_WORLD_SERVICE_* authenticated public-scope connection settings. Engineering history requires explicit diagnostic opt-in; results never prove BFT voting power or runtime application. Watch polls committed snapshots and may skip intermediate transitions. Formal stdout formal_cursor can be saved with jq '.formal_cursor' and resumed with --cursor-file; --cursor-out optionally atomically saves it. Proof cursors are limited to 32 public frames and 16MiB, replay-verified from CURRENT externally pinned configuration; missing history returns HistoryUnavailable. Legacy --cursor remains a small minimum CommitRef. First observed in this run is not the historical first effective commit; source latest-head freshness remains unknown.";

#[derive(Debug, PartialEq)]
struct Options {
    command: String,
    node_id: Option<String>,
    state: String,
    timeout: Duration,
    interval: Duration,
    minimum: Option<CommitRef>,
    engineering: bool,
    cursor_file: Option<PathBuf>,
    cursor_out: Option<PathBuf>,
}
fn parse<'a>(args: impl Iterator<Item = &'a str>) -> Result<Option<Options>, String> {
    let values: Vec<_> = args.collect();
    if values.as_slice() == ["--help"] || values.as_slice() == ["-h"] {
        return Ok(None);
    }
    if values.len() == 2
        && ["list", "status", "wait", "watch"].contains(&values[0])
        && ["--help", "-h"].contains(&values[1])
    {
        return Ok(None);
    }
    let command = values.first().ok_or(HELP)?;
    if !["list", "status", "wait", "watch"].contains(command) {
        return Err(HELP.into());
    }
    let mut flags = BTreeMap::new();
    let mut engineering = false;
    let mut index = 1;
    while index < values.len() {
        let flag = values[index];
        index += 1;
        if flag == "--allow-controlled-prerequisite" {
            if engineering {
                return Err("duplicate engineering opt-in".into());
            }
            engineering = true;
            continue;
        }
        if ![
            "--node-id",
            "--state",
            "--timeout-ms",
            "--interval-ms",
            "--cursor",
            "--cursor-file",
            "--cursor-out",
        ]
        .contains(&flag)
        {
            return Err(format!("unknown membership option: {flag}"));
        }
        let value = *values.get(index).ok_or("missing membership option value")?;
        if value.starts_with("--") || value.trim().is_empty() {
            return Err("missing membership option value".into());
        }
        index += 1;
        if flags.insert(flag, value).is_some() {
            return Err(format!("duplicate membership option: {flag}"));
        }
    }
    let node_id = flags.get("--node-id").map(|id| id.to_string());
    if ["status", "wait"].contains(command) && node_id.is_none() {
        return Err("status/wait requires --node-id".into());
    }
    let state = flags.get("--state").copied().unwrap_or("effective");
    if !["effective", "revoked", "registered", "approved"].contains(&state) {
        return Err("unknown governance state".into());
    }
    if flags.contains_key("--state") && *command != "wait" {
        return Err("--state is only valid for wait".into());
    }
    let milliseconds = |name, default, minimum, maximum| -> Result<u64, String> {
        let value = flags.get(name).map_or(Ok(default), |value| {
            value.parse::<u64>().map_err(|_| format!("invalid {name}"))
        })?;
        if !(minimum..=maximum).contains(&value) {
            return Err(format!("{name} outside supported range"));
        }
        Ok(value)
    };
    let minimum = flags
        .get("--cursor")
        .map(|value| -> Result<CommitRef, String> {
            if value.len() > 16_384 {
                return Err("membership cursor exceeds bound".into());
            }
            let commit: CommitRef =
                serde_json::from_str(value).map_err(|_| "invalid membership cursor".to_string())?;
            commit.validate().map_err(|e| e.to_string())?;
            Ok(commit)
        })
        .transpose()?;
    if flags.contains_key("--cursor") && flags.contains_key("--cursor-file") {
        return Err("--cursor and --cursor-file are mutually exclusive".into());
    }
    let path = |name: &str| -> Result<Option<PathBuf>, String> {
        flags
            .get(name)
            .map(|value| {
                let path = PathBuf::from(value);
                if path.is_absolute() {
                    Ok(path)
                } else {
                    Err(format!("{name} requires an absolute path"))
                }
            })
            .transpose()
    };
    Ok(Some(Options {
        command: command.to_string(),
        node_id,
        state: state.into(),
        timeout: Duration::from_millis(milliseconds("--timeout-ms", 30_000, 1, 3_600_000)?),
        interval: Duration::from_millis(milliseconds("--interval-ms", 500, 100, 10_000)?),
        minimum,
        engineering,
        cursor_file: path("--cursor-file")?,
        cursor_out: path("--cursor-out")?,
    }))
}
fn effective(response: &ReadMembershipResponse, node: &str) -> Option<bool> {
    response
        .effective_registry
        .as_ref()
        .map(|registry| registry.signer_bindings.contains_key(node))
}
fn reached(response: &ReadMembershipResponse, node: &str, state: &str) -> bool {
    let candidate = response
        .selected_candidates
        .get(node)
        .and_then(|id| response.candidates.get(id));
    match state {
        "effective" => effective(response, node) == Some(true),
        "registered" => candidate.is_some(),
        "revoked" => {
            candidate.is_some_and(|record| record.status == MembershipCandidateStatus::Revoked)
        }
        "approved" => candidate.is_some_and(|record| {
            matches!(
                record.status,
                MembershipCandidateStatus::ApprovedCandidate
                    | MembershipCandidateStatus::ProbationReady
                    | MembershipCandidateStatus::Active
            )
        }),
        _ => false,
    }
}
fn output(
    response: &ReadMembershipResponse,
    node: Option<&str>,
    first_observed: &CommitRef,
) -> Value {
    let status = node.map(|node| {
        let candidate = response
            .selected_candidates
            .get(node)
            .and_then(|id| response.candidates.get(id));
        json!({"node_id":node, "selected_candidate":candidate,
            "governance_effective":effective(response,node),
            "effective_signer_public_key":response.effective_registry.as_ref().and_then(|registry| registry.signer_bindings.get(node)),
            "effective_stake":response.effective_registry.as_ref().and_then(|registry| registry.validator_stakes.get(node)),
            "protocol_activation_epoch":candidate.and_then(|record| record.activation_epoch),
            "current_governance_epoch":response.governance_epoch})
    });
    json!({"assurance":"authenticated_service_assertion_not_independent_consensus_proof",
        "configured_role":null,"governance_membership":response,"node_status":status,"cursor":response.commit,
        "first_observed_in_this_run":first_observed,"actual_first_effective_commit":null,
        "applied_consensus_membership":null})
}
fn state_digest(response: &ReadMembershipResponse) -> Result<String, String> {
    authority::request_digest(
        "membership-watch-state",
        &(
            &response.candidates,
            &response.selected_candidates,
            &response.effective_registry_digest,
            &response.history_profile,
        ),
    )
}
fn observed_condition_digest(
    response: &ReadMembershipResponse,
    node: Option<&str>,
) -> Result<String, String> {
    if let Some(node) = node {
        let candidate = response
            .selected_candidates
            .get(node)
            .and_then(|id| response.candidates.get(id));
        authority::request_digest(
            "membership-observed-node-condition",
            &(
                candidate,
                effective(response, node),
                response
                    .effective_registry
                    .as_ref()
                    .and_then(|registry| registry.signer_bindings.get(node)),
                response
                    .effective_registry
                    .as_ref()
                    .and_then(|registry| registry.validator_stakes.get(node)),
            ),
        )
    } else {
        authority::request_digest(
            "membership-observed-collection-condition",
            &(
                &response.candidates,
                &response.selected_candidates,
                &response.effective_registry_digest,
            ),
        )
    }
}
pub(super) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let Some(options) = parse(args)? else {
        println!("{HELP}");
        return Ok(());
    };
    let config = WorldServiceClientConfig::from_env()?
        .ok_or("authenticated WorldService connection settings are required")?;
    if config.scope_id != "public" {
        return Err("membership diagnostics require public scope".into());
    }
    let world = config.expected_world.clone();
    let policy = if options.engineering {
        ReadEvidencePolicy::AllowControlledPrerequisite
    } else {
        ReadEvidencePolicy::Ordinary
    };
    let client = RemoteWorldServiceClient::new_with_evidence_policy(config, policy)
        .map_err(|e| e.to_string())?;
    if (options.cursor_file.is_some() || options.cursor_out.is_some())
        && client.config().formal_read_trust.is_none()
    {
        return Err("formal cursor files require external formal read configuration".into());
    }
    let started = Instant::now();
    let mut minimum = options.minimum;
    if let Some(path) = &options.cursor_file {
        minimum = Some(
            client
                .restore_formal_cursor(&cursor_io::read(path)?)
                .map_err(|error| error.to_string())?,
        );
    }
    let mut saved_cursor_digest = None;
    let mut previous_digest = None;
    let mut previous_condition = None;
    let mut first_observed = None;
    loop {
        let request = ReadMembershipRequest {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: world.clone(),
            min_commit: minimum.clone(),
            fixed_commit: None,
        };
        let mut observation = client.read_membership(request).map_err(|e| e.to_string())?;
        let formal_cursor = if client.config().formal_read_trust.is_some() {
            let cursor = match client.formal_cursor() {
                Ok(cursor) => cursor,
                Err(error)
                    if error
                        .to_string()
                        .contains("cursor evidence budget exhausted") =>
                {
                    let minimum = observation.response().commit.clone();
                    let rebased = client
                        .rebase_formal_cursor(&minimum)
                        .map_err(|error| error.to_string())?;
                    // A publication may advance during the independent C0
                    // read. Observe again from its verified cut before printing
                    // or saving a cursor for a different observation.
                    observation = client
                        .read_membership(ReadMembershipRequest {
                            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                            world: world.clone(),
                            // Rebase already proved this new cut descends
                            // from the original authenticated minimum. Use
                            // its freshly proved exact binding for the next
                            // poll, whose server origin may now be newer.
                            min_commit: Some(rebased.commit),
                            fixed_commit: None,
                        })
                        .map_err(|error| error.to_string())?;
                    client.formal_cursor().map_err(|error| error.to_string())?
                }
                Err(error) => return Err(error.to_string()),
            };
            if cursor.commit != observation.response().commit {
                return Err(
                    "HistoryUnavailable: formal cursor and observation commits differ".into(),
                );
            }
            Some(cursor)
        } else {
            None
        };
        let response = observation.response();
        if let Some(path) = &options.cursor_out {
            let cursor = formal_cursor.as_ref().ok_or("formal cursor unavailable")?;
            let digest = authority::request_digest("formal-membership-cursor-file-v1", cursor)?;
            if saved_cursor_digest.as_ref() != Some(&digest) {
                cursor_io::write(path, cursor)?;
                saved_cursor_digest = Some(digest);
            }
        }
        let digest = state_digest(response)?;
        let changed = previous_digest.as_ref() != Some(&digest);
        let condition = observed_condition_digest(response, options.node_id.as_deref())?;
        if previous_condition.as_ref() != Some(&condition) {
            first_observed = Some(response.commit.clone());
        }
        let matched = options
            .node_id
            .as_deref()
            .is_some_and(|node| reached(response, node, &options.state));
        if (options.command != "wait" || matched) && (options.command != "watch" || changed) {
            let mut value = output(
                response,
                options.node_id.as_deref(),
                first_observed.as_ref().unwrap(),
            );
            if let Some(cursor) = &formal_cursor {
                value["assurance"] = json!(
                    "independently_verified_formal_public_projection_not_runtime_application"
                );
                value["source_freshness"] = json!("unknown");
                value["formal_cursor"] =
                    serde_json::to_value(cursor).map_err(|error| error.to_string())?;
            }
            println!(
                "{}",
                serde_json::to_string(&value).map_err(|e| e.to_string())?
            );
        }
        if ["list", "status"].contains(&options.command.as_str())
            || (options.command == "wait" && matched)
        {
            return Ok(());
        }
        minimum = Some(response.commit.clone());
        previous_digest = Some(digest);
        previous_condition = Some(condition);
        let remaining = options.timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return if options.command == "wait" {
                Err(
                    "governance membership wait timed out; consensus application remains unknown"
                        .into(),
                )
            } else {
                Ok(())
            };
        }
        std::thread::sleep(options.interval.min(remaining));
    }
}

#[cfg(test)]
#[path = "membership_cli_tests.rs"]
mod tests;

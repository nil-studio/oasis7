use oasis7_runtime::{
    Action, CausedBy, DomainEvent, MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
    MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL, RejectReason,
    RestrictedStarterClaimGrantState, RestrictedStarterClaimGrantStatus, World, WorldEvent,
    WorldEventBody,
};
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process;

const DEFAULT_ISSUER_ID: &str = "liveops";

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliCommand {
    Issue(IssueCommand),
    Revoke(RevokeCommand),
    Status(StatusCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IssueCommand {
    world_dir: PathBuf,
    issuer_account_id: String,
    beneficiary_account_id: String,
    amount: u64,
    issuance_reason: String,
    expires_at_epoch: u64,
    dry_run: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RevokeCommand {
    world_dir: PathBuf,
    issuer_account_id: String,
    beneficiary_account_id: String,
    revoke_reason: String,
    dry_run: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StatusCommand {
    world_dir: PathBuf,
    issuer_account_id: String,
    beneficiary_account_id: Option<String>,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ActionEventSummary {
    kind: String,
    detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct GrantSnapshot {
    beneficiary_account_id: String,
    issuer_id: String,
    issuance_reason: String,
    source_treasury_bucket_id: String,
    spend_scope: String,
    issued_amount: u64,
    issued_at_epoch: u64,
    expires_at_epoch: u64,
    status: String,
    status_updated_at_epoch: Option<u64>,
    status_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct StatusReport {
    world_dir: String,
    current_tick: u64,
    ecosystem_treasury_balance: u64,
    restricted_claim_liveops_treasury_balance: u64,
    admin_registry_configured: bool,
    admin_account_ids: Vec<String>,
    issuer_account_id: String,
    issuer_is_allowlisted_admin: bool,
    issuer_has_signer_policy: bool,
    beneficiary_account_id: Option<String>,
    beneficiary_restricted_balance: Option<u64>,
    beneficiary_grant: Option<GrantSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct IssueReport {
    world_dir: String,
    action_id: u64,
    dry_run: bool,
    persisted: bool,
    issuer_account_id: String,
    beneficiary_account_id: String,
    amount: u64,
    issuance_reason: String,
    expires_at_epoch: u64,
    ecosystem_treasury_balance_after: u64,
    restricted_claim_liveops_treasury_balance_after: u64,
    beneficiary_restricted_balance_after: u64,
    beneficiary_grant: Option<GrantSnapshot>,
    action_events: Vec<ActionEventSummary>,
    rejection: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RevokeReport {
    world_dir: String,
    action_id: u64,
    dry_run: bool,
    persisted: bool,
    issuer_account_id: String,
    beneficiary_account_id: String,
    revoke_reason: String,
    ecosystem_treasury_balance_after: u64,
    restricted_claim_liveops_treasury_balance_after: u64,
    beneficiary_restricted_balance_after: u64,
    beneficiary_grant: Option<GrantSnapshot>,
    action_events: Vec<ActionEventSummary>,
    rejection: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandReport {
    Issue(IssueReport),
    Revoke(RevokeReport),
    Status(StatusReport),
}

impl CommandReport {
    fn is_success(&self) -> bool {
        match self {
            Self::Issue(report) => report.rejection.is_none(),
            Self::Revoke(report) => report.rejection.is_none(),
            Self::Status(_) => true,
        }
    }

    fn render_human(&self) -> String {
        match self {
            Self::Issue(report) => render_issue_report(report),
            Self::Revoke(report) => render_revoke_report(report),
            Self::Status(report) => render_status_report(report),
        }
    }
}

fn main() {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if raw_args.is_empty() || raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }

    let command = match parse_command(raw_args.iter().map(String::as_str)) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("{err}");
            print_help();
            process::exit(1);
        }
    };
    let json = command_json(&command);
    let report = match run_command(&command) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("oasis7_liveops_grant_cli failed: {err}");
            process::exit(1);
        }
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report_to_json(&report)).expect("encode report")
        );
    } else {
        println!("{}", report.render_human());
    }
    if !report.is_success() {
        process::exit(2);
    }
}

fn parse_command<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<CliCommand, String> {
    let Some(subcommand) = args.next() else {
        return Err("missing subcommand".to_string());
    };
    match subcommand {
        "issue" => parse_issue_command(args).map(CliCommand::Issue),
        "revoke" => parse_revoke_command(args).map(CliCommand::Revoke),
        "status" => parse_status_command(args).map(CliCommand::Status),
        other => Err(format!("unknown subcommand: {other}")),
    }
}

fn parse_issue_command<'a>(args: impl Iterator<Item = &'a str>) -> Result<IssueCommand, String> {
    let mut world_dir: Option<PathBuf> = None;
    let mut issuer_account_id = DEFAULT_ISSUER_ID.to_string();
    let mut beneficiary_account_id: Option<String> = None;
    let mut amount: Option<u64> = None;
    let mut issuance_reason: Option<String> = None;
    let mut expires_at_epoch: Option<u64> = None;
    let mut dry_run = false;
    let mut json = false;
    let mut iter = args.peekable();

    while let Some(arg) = iter.next() {
        match arg {
            "--world-dir" => {
                world_dir = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--world-dir",
                )?));
            }
            "--issuer-id" => {
                issuer_account_id = parse_required_value(&mut iter, "--issuer-id")?;
            }
            "--beneficiary-account-id" => {
                beneficiary_account_id =
                    Some(parse_required_value(&mut iter, "--beneficiary-account-id")?);
            }
            "--amount" => {
                amount = Some(parse_positive_u64(&mut iter, "--amount")?);
            }
            "--issuance-reason" => {
                issuance_reason = Some(parse_required_value(&mut iter, "--issuance-reason")?);
            }
            "--expires-at-epoch" => {
                expires_at_epoch = Some(parse_positive_u64(&mut iter, "--expires-at-epoch")?);
            }
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            other => return Err(format!("unknown issue option: {other}")),
        }
    }

    Ok(IssueCommand {
        world_dir: world_dir.ok_or_else(|| "--world-dir is required".to_string())?,
        issuer_account_id,
        beneficiary_account_id: beneficiary_account_id
            .ok_or_else(|| "--beneficiary-account-id is required".to_string())?,
        amount: amount.ok_or_else(|| "--amount is required".to_string())?,
        issuance_reason: issuance_reason
            .ok_or_else(|| "--issuance-reason is required".to_string())?,
        expires_at_epoch: expires_at_epoch
            .ok_or_else(|| "--expires-at-epoch is required".to_string())?,
        dry_run,
        json,
    })
}

fn parse_revoke_command<'a>(args: impl Iterator<Item = &'a str>) -> Result<RevokeCommand, String> {
    let mut world_dir: Option<PathBuf> = None;
    let mut issuer_account_id = DEFAULT_ISSUER_ID.to_string();
    let mut beneficiary_account_id: Option<String> = None;
    let mut revoke_reason: Option<String> = None;
    let mut dry_run = false;
    let mut json = false;
    let mut iter = args.peekable();

    while let Some(arg) = iter.next() {
        match arg {
            "--world-dir" => {
                world_dir = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--world-dir",
                )?));
            }
            "--issuer-id" => {
                issuer_account_id = parse_required_value(&mut iter, "--issuer-id")?;
            }
            "--beneficiary-account-id" => {
                beneficiary_account_id =
                    Some(parse_required_value(&mut iter, "--beneficiary-account-id")?);
            }
            "--revoke-reason" => {
                revoke_reason = Some(parse_required_value(&mut iter, "--revoke-reason")?);
            }
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            other => return Err(format!("unknown revoke option: {other}")),
        }
    }

    Ok(RevokeCommand {
        world_dir: world_dir.ok_or_else(|| "--world-dir is required".to_string())?,
        issuer_account_id,
        beneficiary_account_id: beneficiary_account_id
            .ok_or_else(|| "--beneficiary-account-id is required".to_string())?,
        revoke_reason: revoke_reason.ok_or_else(|| "--revoke-reason is required".to_string())?,
        dry_run,
        json,
    })
}

fn parse_status_command<'a>(args: impl Iterator<Item = &'a str>) -> Result<StatusCommand, String> {
    let mut world_dir: Option<PathBuf> = None;
    let mut issuer_account_id = DEFAULT_ISSUER_ID.to_string();
    let mut beneficiary_account_id: Option<String> = None;
    let mut json = false;
    let mut iter = args.peekable();

    while let Some(arg) = iter.next() {
        match arg {
            "--world-dir" => {
                world_dir = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--world-dir",
                )?));
            }
            "--issuer-id" => {
                issuer_account_id = parse_required_value(&mut iter, "--issuer-id")?;
            }
            "--beneficiary-account-id" => {
                beneficiary_account_id =
                    Some(parse_required_value(&mut iter, "--beneficiary-account-id")?);
            }
            "--json" => json = true,
            other => return Err(format!("unknown status option: {other}")),
        }
    }

    Ok(StatusCommand {
        world_dir: world_dir.ok_or_else(|| "--world-dir is required".to_string())?,
        issuer_account_id,
        beneficiary_account_id,
        json,
    })
}

fn parse_required_value<'a>(
    iter: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
    flag: &str,
) -> Result<String, String> {
    iter.next()
        .map(|value| value.to_string())
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_positive_u64<'a>(
    iter: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
    flag: &str,
) -> Result<u64, String> {
    let value = parse_required_value(iter, flag)?;
    value
        .parse::<u64>()
        .ok()
        .filter(|parsed| *parsed > 0)
        .ok_or_else(|| format!("{flag} requires a positive integer"))
}

fn command_json(command: &CliCommand) -> bool {
    match command {
        CliCommand::Issue(command) => command.json,
        CliCommand::Revoke(command) => command.json,
        CliCommand::Status(command) => command.json,
    }
}

fn run_command(command: &CliCommand) -> Result<CommandReport, String> {
    match command {
        CliCommand::Issue(command) => execute_issue(command).map(CommandReport::Issue),
        CliCommand::Revoke(command) => execute_revoke(command).map(CommandReport::Revoke),
        CliCommand::Status(command) => execute_status(command).map(CommandReport::Status),
    }
}

fn execute_issue(command: &IssueCommand) -> Result<IssueReport, String> {
    let mut world = load_existing_world(command.world_dir.as_path())?;
    let journal_len_before = world.journal().events.len();
    let action_id = world.submit_action(Action::IssueRestrictedStarterClaimGrant {
        issuer_account_id: command.issuer_account_id.clone(),
        beneficiary_account_id: command.beneficiary_account_id.clone(),
        amount: command.amount,
        issuance_reason: command.issuance_reason.clone(),
        expires_at_epoch: command.expires_at_epoch,
    });
    world
        .step()
        .map_err(|err| format!("execute issue restricted grant action failed: {err:?}"))?;
    let action_events = collect_action_events(&world, journal_len_before, action_id);
    let rejection = find_action_rejection(&action_events);
    let persisted = rejection.is_none() && !command.dry_run;
    if persisted {
        world
            .save_to_dir(command.world_dir.as_path())
            .map_err(|err| format!("save world {} failed: {err:?}", command.world_dir.display()))?;
    }
    Ok(IssueReport {
        world_dir: command.world_dir.display().to_string(),
        action_id,
        dry_run: command.dry_run,
        persisted,
        issuer_account_id: command.issuer_account_id.clone(),
        beneficiary_account_id: command.beneficiary_account_id.clone(),
        amount: command.amount,
        issuance_reason: command.issuance_reason.clone(),
        expires_at_epoch: command.expires_at_epoch,
        ecosystem_treasury_balance_after: world
            .main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        restricted_claim_liveops_treasury_balance_after: world.main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
        ),
        beneficiary_restricted_balance_after: world
            .main_token_restricted_starter_claim_balance(command.beneficiary_account_id.as_str()),
        beneficiary_grant: world
            .restricted_starter_claim_grant(command.beneficiary_account_id.as_str())
            .map(grant_snapshot_from_state),
        action_events: summarize_action_events(&action_events),
        rejection,
    })
}

fn execute_revoke(command: &RevokeCommand) -> Result<RevokeReport, String> {
    let mut world = load_existing_world(command.world_dir.as_path())?;
    let journal_len_before = world.journal().events.len();
    let action_id = world.submit_action(Action::RevokeRestrictedStarterClaimGrant {
        issuer_account_id: command.issuer_account_id.clone(),
        beneficiary_account_id: command.beneficiary_account_id.clone(),
        revoke_reason: command.revoke_reason.clone(),
    });
    world
        .step()
        .map_err(|err| format!("execute revoke restricted grant action failed: {err:?}"))?;
    let action_events = collect_action_events(&world, journal_len_before, action_id);
    let rejection = find_action_rejection(&action_events);
    let persisted = rejection.is_none() && !command.dry_run;
    if persisted {
        world
            .save_to_dir(command.world_dir.as_path())
            .map_err(|err| format!("save world {} failed: {err:?}", command.world_dir.display()))?;
    }
    Ok(RevokeReport {
        world_dir: command.world_dir.display().to_string(),
        action_id,
        dry_run: command.dry_run,
        persisted,
        issuer_account_id: command.issuer_account_id.clone(),
        beneficiary_account_id: command.beneficiary_account_id.clone(),
        revoke_reason: command.revoke_reason.clone(),
        ecosystem_treasury_balance_after: world
            .main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        restricted_claim_liveops_treasury_balance_after: world.main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
        ),
        beneficiary_restricted_balance_after: world
            .main_token_restricted_starter_claim_balance(command.beneficiary_account_id.as_str()),
        beneficiary_grant: world
            .restricted_starter_claim_grant(command.beneficiary_account_id.as_str())
            .map(grant_snapshot_from_state),
        action_events: summarize_action_events(&action_events),
        rejection,
    })
}

fn execute_status(command: &StatusCommand) -> Result<StatusReport, String> {
    let world = load_existing_world(command.world_dir.as_path())?;
    let registry = world.governance_main_token_controller_registry();
    let admin_account_ids = registry
        .map(|registry| {
            registry
                .restricted_starter_claim_admin_account_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let issuer_is_allowlisted_admin = registry.is_some_and(|registry| {
        registry
            .restricted_starter_claim_admin_account_ids
            .contains(command.issuer_account_id.as_str())
    });
    let issuer_has_signer_policy = registry.is_some_and(|registry| {
        registry
            .controller_signer_policies
            .contains_key(command.issuer_account_id.as_str())
    });
    let beneficiary_restricted_balance = command
        .beneficiary_account_id
        .as_ref()
        .map(|account_id| world.main_token_restricted_starter_claim_balance(account_id.as_str()));
    let beneficiary_grant = command
        .beneficiary_account_id
        .as_ref()
        .and_then(|account_id| world.restricted_starter_claim_grant(account_id.as_str()))
        .map(grant_snapshot_from_state);
    Ok(StatusReport {
        world_dir: command.world_dir.display().to_string(),
        current_tick: world.state().time,
        ecosystem_treasury_balance: world
            .main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        restricted_claim_liveops_treasury_balance: world.main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
        ),
        admin_registry_configured: registry.is_some(),
        admin_account_ids,
        issuer_account_id: command.issuer_account_id.clone(),
        issuer_is_allowlisted_admin,
        issuer_has_signer_policy,
        beneficiary_account_id: command.beneficiary_account_id.clone(),
        beneficiary_restricted_balance,
        beneficiary_grant,
    })
}

fn load_existing_world(world_dir: &Path) -> Result<World, String> {
    let snapshot_path = world_dir.join("snapshot.json");
    let journal_path = world_dir.join("journal.json");
    if !snapshot_path.exists() || !journal_path.exists() {
        return Err(format!(
            "world_dir {} is missing snapshot.json or journal.json",
            world_dir.display()
        ));
    }
    oasis7::runtime_host::world::load_from_dir(world_dir)
        .map_err(|err| format!("load world {} failed: {err:?}", world_dir.display()))
}

fn collect_action_events(world: &World, start_idx: usize, action_id: u64) -> Vec<WorldEvent> {
    world
        .journal()
        .events
        .iter()
        .skip(start_idx)
        .filter(|event| matches!(event.caused_by, Some(CausedBy::Action(id)) if id == action_id))
        .cloned()
        .collect()
}

fn summarize_action_events(events: &[WorldEvent]) -> Vec<ActionEventSummary> {
    events
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionAccepted { .. }) => None,
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
                Some(ActionEventSummary {
                    kind: "action_rejected".to_string(),
                    detail: format_reject_reason(reason),
                })
            }
            WorldEventBody::Domain(DomainEvent::RestrictedStarterClaimGrantIssued {
                amount,
                issuance_reason,
                expires_at_epoch,
                ..
            }) => Some(ActionEventSummary {
                kind: "restricted_grant_issued".to_string(),
                detail: format!(
                    "amount={amount} issuance_reason={issuance_reason} expires_at_epoch={expires_at_epoch}"
                ),
            }),
            WorldEventBody::Domain(DomainEvent::RestrictedStarterClaimGrantRevoked {
                revoked_amount,
                revoke_reason,
                ..
            }) => Some(ActionEventSummary {
                kind: "restricted_grant_revoked".to_string(),
                detail: format!(
                    "revoked_amount={revoked_amount} revoke_reason={revoke_reason}"
                ),
            }),
            _ => None,
        })
        .collect()
}

fn find_action_rejection(events: &[WorldEvent]) -> Option<String> {
    events.iter().find_map(|event| match &event.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            Some(format_reject_reason(reason))
        }
        _ => None,
    })
}

fn format_reject_reason(reason: &RejectReason) -> String {
    match reason {
        RejectReason::RuleDenied { notes } if !notes.is_empty() => notes.join(" | "),
        _ => serde_json::to_string(reason).unwrap_or_else(|_| format!("{reason:?}")),
    }
}

fn grant_snapshot_from_state(grant: &RestrictedStarterClaimGrantState) -> GrantSnapshot {
    GrantSnapshot {
        beneficiary_account_id: grant.beneficiary_account_id.clone(),
        issuer_id: grant.issuer_id.clone(),
        issuance_reason: grant.issuance_reason.clone(),
        source_treasury_bucket_id: grant.source_treasury_bucket_id.clone(),
        spend_scope: grant.spend_scope.clone(),
        issued_amount: grant.issued_amount,
        issued_at_epoch: grant.issued_at_epoch,
        expires_at_epoch: grant.expires_at_epoch,
        status: grant_status_label(&grant.status).to_string(),
        status_updated_at_epoch: grant.status_updated_at_epoch,
        status_reason: grant.status_reason.clone(),
    }
}

fn grant_status_label(status: &RestrictedStarterClaimGrantStatus) -> &'static str {
    match status {
        RestrictedStarterClaimGrantStatus::Issued => "issued",
        RestrictedStarterClaimGrantStatus::Expired => "expired",
        RestrictedStarterClaimGrantStatus::Revoked => "revoked",
    }
}

fn report_to_json(report: &CommandReport) -> serde_json::Value {
    match report {
        CommandReport::Issue(report) => serde_json::to_value(report).expect("encode issue report"),
        CommandReport::Revoke(report) => {
            serde_json::to_value(report).expect("encode revoke report")
        }
        CommandReport::Status(report) => {
            serde_json::to_value(report).expect("encode status report")
        }
    }
}

fn render_issue_report(report: &IssueReport) -> String {
    let header = if report.rejection.is_some() {
        "issue rejected"
    } else if report.dry_run {
        "issue dry-run ok"
    } else {
        "issue ok"
    };
    let mut lines = vec![
        header.to_string(),
        format!("world_dir: {}", report.world_dir),
        format!("action_id: {}", report.action_id),
        format!("issuer_id: {}", report.issuer_account_id),
        format!("beneficiary_account_id: {}", report.beneficiary_account_id),
        format!("amount: {}", report.amount),
        format!("issuance_reason: {}", report.issuance_reason),
        format!("expires_at_epoch: {}", report.expires_at_epoch),
        format!(
            "ecosystem_treasury_balance_after: {}",
            report.ecosystem_treasury_balance_after
        ),
        format!(
            "restricted_claim_liveops_treasury_balance_after: {}",
            report.restricted_claim_liveops_treasury_balance_after
        ),
        format!(
            "beneficiary_restricted_balance_after: {}",
            report.beneficiary_restricted_balance_after
        ),
    ];
    if let Some(grant) = &report.beneficiary_grant {
        lines.push(format!("beneficiary_grant_status: {}", grant.status));
    }
    if let Some(rejection) = &report.rejection {
        lines.push(format!("rejection: {rejection}"));
    }
    for event in &report.action_events {
        lines.push(format!("event.{}: {}", event.kind, event.detail));
    }
    lines.join("\n")
}

fn render_revoke_report(report: &RevokeReport) -> String {
    let header = if report.rejection.is_some() {
        "revoke rejected"
    } else if report.dry_run {
        "revoke dry-run ok"
    } else {
        "revoke ok"
    };
    let mut lines = vec![
        header.to_string(),
        format!("world_dir: {}", report.world_dir),
        format!("action_id: {}", report.action_id),
        format!("issuer_id: {}", report.issuer_account_id),
        format!("beneficiary_account_id: {}", report.beneficiary_account_id),
        format!("revoke_reason: {}", report.revoke_reason),
        format!(
            "ecosystem_treasury_balance_after: {}",
            report.ecosystem_treasury_balance_after
        ),
        format!(
            "restricted_claim_liveops_treasury_balance_after: {}",
            report.restricted_claim_liveops_treasury_balance_after
        ),
        format!(
            "beneficiary_restricted_balance_after: {}",
            report.beneficiary_restricted_balance_after
        ),
    ];
    if let Some(grant) = &report.beneficiary_grant {
        lines.push(format!("beneficiary_grant_status: {}", grant.status));
    }
    if let Some(rejection) = &report.rejection {
        lines.push(format!("rejection: {rejection}"));
    }
    for event in &report.action_events {
        lines.push(format!("event.{}: {}", event.kind, event.detail));
    }
    lines.join("\n")
}

fn render_status_report(report: &StatusReport) -> String {
    let mut lines = vec![
        "status".to_string(),
        format!("world_dir: {}", report.world_dir),
        format!("current_tick: {}", report.current_tick),
        format!(
            "ecosystem_treasury_balance: {}",
            report.ecosystem_treasury_balance
        ),
        format!(
            "restricted_claim_liveops_treasury_balance: {}",
            report.restricted_claim_liveops_treasury_balance
        ),
        format!(
            "admin_registry_configured: {}",
            report.admin_registry_configured
        ),
        format!(
            "admin_account_ids: {}",
            if report.admin_account_ids.is_empty() {
                "(empty)".to_string()
            } else {
                report.admin_account_ids.join(", ")
            }
        ),
        format!("issuer_id: {}", report.issuer_account_id),
        format!(
            "issuer_is_allowlisted_admin: {}",
            report.issuer_is_allowlisted_admin
        ),
        format!(
            "issuer_has_signer_policy: {}",
            report.issuer_has_signer_policy
        ),
    ];
    if let Some(beneficiary_account_id) = &report.beneficiary_account_id {
        lines.push(format!("beneficiary_account_id: {beneficiary_account_id}"));
        lines.push(format!(
            "beneficiary_restricted_balance: {}",
            report.beneficiary_restricted_balance.unwrap_or(0)
        ));
        if let Some(grant) = &report.beneficiary_grant {
            lines.push(format!("beneficiary_grant_status: {}", grant.status));
            lines.push(format!("beneficiary_grant_amount: {}", grant.issued_amount));
            lines.push(format!(
                "beneficiary_grant_expires_at_epoch: {}",
                grant.expires_at_epoch
            ));
            lines.push(format!(
                "beneficiary_grant_issuance_reason: {}",
                grant.issuance_reason
            ));
        } else {
            lines.push("beneficiary_grant_status: (none)".to_string());
        }
    }
    lines.join("\n")
}

fn print_help() {
    eprintln!("Usage:");
    eprintln!(
        "  oasis7_liveops_grant_cli issue --world-dir <dir> --beneficiary-account-id <account> --amount <n> --issuance-reason <reason> --expires-at-epoch <epoch> [--issuer-id <id>] [--dry-run] [--json]"
    );
    eprintln!(
        "  oasis7_liveops_grant_cli revoke --world-dir <dir> --beneficiary-account-id <account> --revoke-reason <reason> [--issuer-id <id>] [--dry-run] [--json]"
    );
    eprintln!(
        "  oasis7_liveops_grant_cli status --world-dir <dir> [--issuer-id <id>] [--beneficiary-account-id <account>] [--json]"
    );
    eprintln!();
    eprintln!("Notes:");
    eprintln!("  - default issuer_id is `{DEFAULT_ISSUER_ID}`");
    eprintln!(
        "  - this CLI is for daily liveops grant issue/revoke/status only; admin roster updates must continue to use controller-governed registry actions"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_runtime::{
        GovernanceMainTokenControllerRegistry, GovernanceThresholdSignerPolicy,
        MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
        MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL, MainTokenSupplyState,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(prefix: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let sequence = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-liveops-grant-cli-{prefix}-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn sample_policy(public_key_hex: &str) -> GovernanceThresholdSignerPolicy {
        GovernanceThresholdSignerPolicy {
            threshold: 1,
            allowed_public_keys: BTreeSet::from([public_key_hex.to_string()]),
        }
    }

    fn sample_registry() -> GovernanceMainTokenControllerRegistry {
        GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.to_string(),
                "msig.ecosystem_governance.v1".to_string(),
            )]),
            restricted_starter_claim_admin_account_ids: BTreeSet::from([
                DEFAULT_ISSUER_ID.to_string()
            ]),
            controller_signer_policies: BTreeMap::from([
                (
                    "msig.genesis.v1".to_string(),
                    sample_policy(
                        "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30",
                    ),
                ),
                (
                    "msig.ecosystem_governance.v1".to_string(),
                    sample_policy(
                        "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20",
                    ),
                ),
                (
                    DEFAULT_ISSUER_ID.to_string(),
                    sample_policy(
                        "10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb",
                    ),
                ),
            ]),
        }
    }

    fn sample_world_dir() -> PathBuf {
        let world_dir = temp_dir("world");
        let mut world = oasis7::runtime_host::world::new();
        world.set_main_token_supply(MainTokenSupplyState {
            total_supply: 10_000,
            circulating_supply: 0,
            total_issued: 10_000,
            total_burned: 0,
        });
        world
            .set_governance_main_token_controller_registry(sample_registry())
            .expect("set controller registry");
        world
            .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL, 1_000)
            .expect("set ecosystem treasury");
        world
            .set_main_token_treasury_balance(
                MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
                600,
            )
            .expect("set restricted liveops treasury");
        world.save_to_dir(world_dir.as_path()).expect("save world");
        world_dir
    }

    #[test]
    fn parse_issue_command_defaults_issuer_to_liveops() {
        let command = parse_command(
            [
                "issue",
                "--world-dir",
                "/tmp/world",
                "--beneficiary-account-id",
                "oc:pk:test",
                "--amount",
                "300",
                "--issuance-reason",
                "preview_allowlist",
                "--expires-at-epoch",
                "5",
            ]
            .into_iter(),
        )
        .expect("parse command");
        let CliCommand::Issue(command) = command else {
            panic!("expected issue command");
        };
        assert_eq!(command.issuer_account_id, DEFAULT_ISSUER_ID);
    }

    #[test]
    fn issue_command_applies_and_persists_restricted_grant() {
        let world_dir = sample_world_dir();
        let command = IssueCommand {
            world_dir: world_dir.clone(),
            issuer_account_id: DEFAULT_ISSUER_ID.to_string(),
            beneficiary_account_id: "oc:pk:beneficiary".to_string(),
            amount: 300,
            issuance_reason: "preview_allowlist".to_string(),
            expires_at_epoch: 5,
            dry_run: false,
            json: false,
        };

        let report = execute_issue(&command).expect("issue should succeed");
        assert!(report.rejection.is_none(), "{report:?}");
        assert!(report.persisted);
        assert_eq!(report.beneficiary_restricted_balance_after, 300);
        assert_eq!(report.restricted_claim_liveops_treasury_balance_after, 300);
        assert!(
            report
                .action_events
                .iter()
                .any(|event| event.kind == "restricted_grant_issued")
        );

        let restored =
            oasis7::runtime_host::world::load_from_dir(world_dir.as_path()).expect("reload world");
        let grant = restored
            .restricted_starter_claim_grant("oc:pk:beneficiary")
            .expect("grant exists");
        assert_eq!(grant.issued_amount, 300);
        assert_eq!(grant.status, RestrictedStarterClaimGrantStatus::Issued);
    }

    #[test]
    fn revoke_command_revokes_existing_grant() {
        let world_dir = sample_world_dir();
        execute_issue(&IssueCommand {
            world_dir: world_dir.clone(),
            issuer_account_id: DEFAULT_ISSUER_ID.to_string(),
            beneficiary_account_id: "oc:pk:beneficiary".to_string(),
            amount: 300,
            issuance_reason: "qa_seed".to_string(),
            expires_at_epoch: 6,
            dry_run: false,
            json: false,
        })
        .expect("seed grant");

        let report = execute_revoke(&RevokeCommand {
            world_dir: world_dir.clone(),
            issuer_account_id: DEFAULT_ISSUER_ID.to_string(),
            beneficiary_account_id: "oc:pk:beneficiary".to_string(),
            revoke_reason: "qa_window_closed".to_string(),
            dry_run: false,
            json: false,
        })
        .expect("revoke should succeed");
        assert!(report.rejection.is_none(), "{report:?}");
        assert!(report.persisted);
        assert_eq!(report.restricted_claim_liveops_treasury_balance_after, 600);
        assert!(
            report
                .action_events
                .iter()
                .any(|event| event.kind == "restricted_grant_revoked")
        );

        let restored =
            oasis7::runtime_host::world::load_from_dir(world_dir.as_path()).expect("reload world");
        let grant = restored
            .restricted_starter_claim_grant("oc:pk:beneficiary")
            .expect("grant exists");
        assert_eq!(grant.status, RestrictedStarterClaimGrantStatus::Revoked);
        assert_eq!(grant.status_reason.as_deref(), Some("qa_window_closed"));
    }

    #[test]
    fn status_command_reports_admin_and_beneficiary_state() {
        let world_dir = sample_world_dir();
        execute_issue(&IssueCommand {
            world_dir: world_dir.clone(),
            issuer_account_id: DEFAULT_ISSUER_ID.to_string(),
            beneficiary_account_id: "oc:pk:beneficiary".to_string(),
            amount: 300,
            issuance_reason: "liveops_campaign".to_string(),
            expires_at_epoch: 7,
            dry_run: false,
            json: false,
        })
        .expect("seed grant");

        let report = execute_status(&StatusCommand {
            world_dir,
            issuer_account_id: DEFAULT_ISSUER_ID.to_string(),
            beneficiary_account_id: Some("oc:pk:beneficiary".to_string()),
            json: false,
        })
        .expect("status");
        assert!(report.admin_registry_configured);
        assert!(report.issuer_is_allowlisted_admin);
        assert!(report.issuer_has_signer_policy);
        assert_eq!(report.restricted_claim_liveops_treasury_balance, 300);
        assert_eq!(
            report.beneficiary_restricted_balance,
            Some(300),
            "{report:?}"
        );
        assert_eq!(
            report
                .beneficiary_grant
                .as_ref()
                .map(|grant| grant.status.as_str()),
            Some("issued")
        );
    }
}

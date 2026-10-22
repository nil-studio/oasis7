use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};
use std::process;

use oasis7_runtime::{
    MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL, MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE,
    MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD,
};
use serde::Deserialize;

use oasis7::governance_audit_report as audit_report;
#[path = "oasis7_governance_registry_audit/manifest_digest.rs"]
mod manifest_digest;
use audit_report::{GovernanceRegistryAuditReport, RollbackAuthorityAuditRow, audit_row};
use manifest_digest::audited_manifest_digest;

const DEFAULT_FINALITY_SLOT_ID: &str = "governance.finality.v1";
const DEFAULT_EXPECTED_THRESHOLD: u16 = 2;
const ROLLBACK_ON_CALL_SLOT_ID: &str = "ops.rollback.on_call.v1";
const ROLLBACK_GOVERNANCE_SLOT_ID: &str = "governance.rollback.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    world_dir: PathBuf,
    public_manifest: Option<PathBuf>,
    finality_slot_id: String,
    default_expected_threshold: u16,
    strict_manifest_match: bool,
    require_single_failure_tolerance: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct PublicManifestEntry {
    slot_id: String,
    signer_id: String,
    scheme: String,
    public_key_hex: String,
    #[serde(default)]
    threshold: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum PublicManifest {
    Entries(Vec<PublicManifestEntry>),
    Document { entries: Vec<PublicManifestEntry> },
}

impl PublicManifest {
    fn into_entries(self) -> Vec<PublicManifestEntry> {
        match self {
            Self::Entries(entries) | Self::Document { entries } => entries,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManifestSlotExpectation {
    threshold: u16,
    public_keys: BTreeSet<String>,
    signer_ids: BTreeSet<String>,
}

fn main() {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }
    let options = match parse_options(raw_args.iter().map(|arg| arg.as_str())) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}");
            print_help();
            process::exit(1);
        }
    };

    let report = match build_audit_report(&options) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("oasis7_governance_registry_audit failed: {err}");
            process::exit(1);
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("encode audit report")
    );
    let errors = validate_audit_report(&options, &report);
    if !errors.is_empty() {
        for error in errors {
            eprintln!("{error}");
        }
        process::exit(2);
    }
}

fn build_audit_report(options: &CliOptions) -> Result<GovernanceRegistryAuditReport, String> {
    let world = oasis7::runtime_host::world::load_from_dir(options.world_dir.as_path())
        .map_err(|err| format!("load world {} failed: {err:?}", options.world_dir.display()))?;
    let audited_manifest_digest = audited_manifest_digest(options.public_manifest.as_deref())?;
    let manifest_slots = if let Some(path) = options.public_manifest.as_ref() {
        Some(load_manifest_slot_expectations(
            path.as_path(),
            options.default_expected_threshold,
        )?)
    } else {
        None
    };

    let finality_registry = world
        .resolve_governance_effective_finality_signer_registry()
        .map_err(|err| {
            format!("resolve effective governance finality signer registry failed: {err:?}")
        })?
        .ok_or_else(|| "world is missing governance finality signer registry".to_string())?;
    let finality_keys = finality_registry
        .signer_bindings
        .values()
        .cloned()
        .collect::<BTreeSet<String>>();
    let finality_expected_threshold = manifest_slots
        .as_ref()
        .and_then(|slots| slots.get(options.finality_slot_id.as_str()))
        .map(|slot| slot.threshold)
        .unwrap_or(options.default_expected_threshold);
    let finality_manifest_match = manifest_slots.as_ref().map(|slots| {
        slots
            .get(options.finality_slot_id.as_str())
            .is_some_and(|slot| {
                slot.public_keys == finality_keys && slot.threshold == finality_registry.threshold
            })
    });
    let finality = audit_row(
        finality_registry.slot_id.as_str(),
        finality_registry.threshold,
        finality_registry.signer_bindings.len(),
        finality_expected_threshold,
        finality_manifest_match,
    );

    let controller_registry = world
        .governance_main_token_controller_registry()
        .ok_or_else(|| "world is missing governance main token controller registry".to_string())?;
    let mut controllers = controller_registry
        .controller_signer_policies
        .iter()
        .map(|(slot_id, policy)| {
            let expected_threshold = manifest_slots
                .as_ref()
                .and_then(|slots| slots.get(slot_id.as_str()))
                .map(|slot| slot.threshold)
                .unwrap_or(options.default_expected_threshold);
            let manifest_match = manifest_slots.as_ref().map(|slots| {
                slots.get(slot_id.as_str()).is_some_and(|slot| {
                    slot.public_keys == policy.allowed_public_keys
                        && slot.threshold == policy.threshold
                })
            });
            audit_row(
                slot_id.as_str(),
                policy.threshold,
                policy.allowed_public_keys.len(),
                expected_threshold,
                manifest_match,
            )
        })
        .collect::<Vec<_>>();
    controllers.sort_by(|left, right| left.slot_id.cmp(&right.slot_id));

    let rollback_registry = &world.snapshot().rollback_authority_registry;
    let mut rollback_blockers = Vec::new();
    let rollback_authorities = [
        (
            ROLLBACK_ON_CALL_SLOT_ID,
            "on_call",
            oasis7_runtime::RollbackAuthorityRole::OnCall,
        ),
        (
            ROLLBACK_GOVERNANCE_SLOT_ID,
            "governance",
            oasis7_runtime::RollbackAuthorityRole::Governance,
        ),
    ]
    .into_iter()
    .map(|(slot_id, role_label, role)| {
        let record = rollback_registry
            .records()
            .find(|record| record.role == role);
        let configured = record.is_some();
        let active = record.is_some_and(|record| record.active);
        let manifest_match = manifest_slots.as_ref().map(|slots| {
            let Some(expected) = slots.get(slot_id) else {
                return false;
            };
            record.is_some_and(|record| {
                expected.threshold == 1
                    && expected.signer_ids.len() == 1
                    && expected.public_keys.len() == 1
                    && expected.signer_ids.contains(record.authority_id.as_str())
                    && expected
                        .public_keys
                        .contains(record.public_key_hex.as_str())
            })
        });
        let status = if !configured {
            "missing".to_string()
        } else if !active {
            "inactive".to_string()
        } else if manifest_match == Some(false) {
            "manifest_mismatch".to_string()
        } else {
            "ready".to_string()
        };
        if status != "ready" {
            rollback_blockers.push(format!("{slot_id}:{status}"));
        }
        RollbackAuthorityAuditRow {
            slot_id: slot_id.to_string(),
            role: role_label.to_string(),
            configured,
            active,
            threshold: 1,
            manifest_match,
            status,
        }
    })
    .collect::<Vec<_>>();

    let overall_single_failure_tolerance_pass = finality.single_failure_tolerant
        && controllers.iter().all(|row| row.single_failure_tolerant);
    let threshold_expectation_pass = finality.threshold_matches_expectation
        && controllers
            .iter()
            .all(|row| row.threshold_matches_expectation);
    let manifest_match_pass = manifest_slots.as_ref().map(|slots| {
        let world_slot_ids = std::iter::once(finality.slot_id.clone())
            .chain(controllers.iter().map(|row| row.slot_id.clone()))
            .collect::<BTreeSet<String>>();
        let manifest_slot_ids = slots
            .keys()
            .filter(|slot_id| {
                slot_id.as_str() != ROLLBACK_ON_CALL_SLOT_ID
                    && slot_id.as_str() != ROLLBACK_GOVERNANCE_SLOT_ID
            })
            .cloned()
            .collect::<BTreeSet<String>>();
        let rollback_registry = &world.snapshot().rollback_authority_registry;
        let rollback_slots_present = slots.contains_key(ROLLBACK_ON_CALL_SLOT_ID)
            && slots.contains_key(ROLLBACK_GOVERNANCE_SLOT_ID);
        let rollback_match = if !options.strict_manifest_match
            && !rollback_slots_present
            && rollback_registry.is_empty()
        {
            true
        } else if rollback_slots_present {
            [
                (
                    ROLLBACK_ON_CALL_SLOT_ID,
                    oasis7_runtime::RollbackAuthorityRole::OnCall,
                ),
                (
                    ROLLBACK_GOVERNANCE_SLOT_ID,
                    oasis7_runtime::RollbackAuthorityRole::Governance,
                ),
            ]
            .into_iter()
            .all(|(slot_id, role)| {
                let Some(expected) = slots.get(slot_id) else {
                    return false;
                };
                if expected.threshold != 1
                    || expected.public_keys.len() != 1
                    || expected.signer_ids.len() != 1
                {
                    return false;
                }
                let expected_id = expected.signer_ids.iter().next().expect("one signer id");
                rollback_registry.get(expected_id).is_some_and(|record| {
                    record.role == role
                        && record.active
                        && expected
                            .public_keys
                            .contains(record.public_key_hex.as_str())
                })
            }) && rollback_registry.len() == 2
        } else {
            false
        };
        world_slot_ids == manifest_slot_ids
            && finality.manifest_match.unwrap_or(false)
            && controllers
                .iter()
                .all(|row| row.manifest_match.unwrap_or(false))
            && rollback_match
    });
    let overall_status = if !rollback_blockers.is_empty() {
        "rollback_authority_blocked".to_string()
    } else if !threshold_expectation_pass {
        "threshold_mismatch".to_string()
    } else if overall_single_failure_tolerance_pass && manifest_match_pass.unwrap_or(true) {
        "ready_for_ops_drill".to_string()
    } else if !overall_single_failure_tolerance_pass {
        "failover_blocked".to_string()
    } else {
        "manifest_mismatch".to_string()
    };

    Ok(GovernanceRegistryAuditReport {
        world_dir: options.world_dir.display().to_string(),
        audited_manifest_digest,
        finality,
        controllers,
        rollback_authorities,
        rollback_blockers,
        overall_single_failure_tolerance_pass,
        manifest_match_pass,
        overall_status,
    })
}

fn validate_audit_report(
    options: &CliOptions,
    report: &GovernanceRegistryAuditReport,
) -> Vec<String> {
    let mut errors = Vec::new();
    if !report.finality.threshold_matches_expectation
        || report
            .controllers
            .iter()
            .any(|row| !row.threshold_matches_expectation)
    {
        errors.push("governance registry audit failed: at least one slot threshold does not match the expected threshold set".to_string());
    }
    if options.require_single_failure_tolerance && !report.overall_single_failure_tolerance_pass {
        errors.push(
            "governance registry audit failed: at least one slot cannot tolerate a single signer failure"
                .to_string(),
        );
    }
    if options.strict_manifest_match && report.manifest_match_pass != Some(true) {
        errors.push(
            "governance registry audit failed: world-state registry does not exactly match the provided public manifest"
                .to_string(),
        );
    }
    if !report.rollback_blockers.is_empty() {
        errors.push(format!(
            "governance registry audit failed: rollback authority blockers: {}",
            report.rollback_blockers.join(",")
        ));
    }
    errors
}

fn load_manifest_slot_expectations(
    path: &Path,
    default_threshold: u16,
) -> Result<BTreeMap<String, ManifestSlotExpectation>, String> {
    let bytes = std::fs::read(path)
        .map_err(|err| format!("read public manifest {} failed: {err}", path.display()))?;
    let entries = serde_json::from_slice::<PublicManifest>(bytes.as_slice())
        .map_err(|err| format!("decode public manifest {} failed: {err}", path.display()))?
        .into_entries();
    let mut slots = BTreeMap::new();
    for entry in entries {
        validate_manifest_entry(&entry)?;
        let slot_id = entry.slot_id.trim().to_string();
        let resolved_threshold = entry.threshold.unwrap_or(default_threshold);
        let slot = slots
            .entry(slot_id.clone())
            .or_insert_with(|| ManifestSlotExpectation {
                threshold: resolved_threshold,
                public_keys: BTreeSet::new(),
                signer_ids: BTreeSet::new(),
            });
        if slot.threshold != resolved_threshold {
            return Err(format!(
                "manifest slot threshold mismatch slot_id={} expected={} actual={resolved_threshold}",
                entry.slot_id, slot.threshold
            ));
        }
        slot.public_keys
            .insert(entry.public_key_hex.trim().to_string());
        slot.signer_ids.insert(entry.signer_id.trim().to_string());
    }
    Ok(slots)
}

fn validate_manifest_entry(entry: &PublicManifestEntry) -> Result<(), String> {
    if entry.slot_id.trim().is_empty() || entry.signer_id.trim().is_empty() {
        return Err("manifest entry slot_id/signer_id cannot be empty".to_string());
    }
    if !entry.scheme.trim().eq_ignore_ascii_case("ed25519") {
        return Err(format!(
            "unsupported signer scheme slot_id={} signer_id={} scheme={}",
            entry.slot_id, entry.signer_id, entry.scheme
        ));
    }
    let public_key_hex = entry.public_key_hex.trim();
    if public_key_hex.len() != 64 || !public_key_hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "manifest entry public_key_hex must be 32-byte hex slot_id={} signer_id={}",
            entry.slot_id, entry.signer_id
        ));
    }
    if entry.threshold.is_some_and(|value| value == 0) {
        return Err(format!(
            "manifest entry threshold must be > 0 slot_id={} signer_id={}",
            entry.slot_id, entry.signer_id
        ));
    }
    Ok(())
}

fn parse_options<'a>(args: impl Iterator<Item = &'a str>) -> Result<CliOptions, String> {
    let mut world_dir = None;
    let mut public_manifest = None;
    let mut finality_slot_id = DEFAULT_FINALITY_SLOT_ID.to_string();
    let mut default_expected_threshold = DEFAULT_EXPECTED_THRESHOLD;
    let mut strict_manifest_match = false;
    let mut require_single_failure_tolerance = false;
    let mut iter = args.peekable();
    while let Some(arg) = iter.next() {
        match arg {
            "--world-dir" => {
                world_dir = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--world-dir",
                )?));
            }
            "--public-manifest" => {
                public_manifest = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--public-manifest",
                )?));
            }
            "--finality-slot-id" => {
                finality_slot_id = parse_required_value(&mut iter, "--finality-slot-id")?;
            }
            "--expected-threshold" => {
                default_expected_threshold =
                    parse_required_value(&mut iter, "--expected-threshold")?
                        .parse::<u16>()
                        .ok()
                        .filter(|value| *value > 0)
                        .ok_or_else(|| {
                            "--expected-threshold requires a positive integer".to_string()
                        })?;
            }
            "--strict-manifest-match" => {
                strict_manifest_match = true;
            }
            "--require-single-failure-tolerance" => {
                require_single_failure_tolerance = true;
            }
            _ => return Err(format!("unknown option: {arg}")),
        }
    }
    Ok(CliOptions {
        world_dir: world_dir.ok_or_else(|| "--world-dir is required".to_string())?,
        public_manifest,
        finality_slot_id,
        default_expected_threshold,
        strict_manifest_match,
        require_single_failure_tolerance,
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

fn print_help() {
    eprintln!(
        "Usage: oasis7_governance_registry_audit --world-dir <dir> [--public-manifest <file>] [--finality-slot-id <slot>] [--expected-threshold <n>] [--strict-manifest-match] [--require-single-failure-tolerance]"
    );
    eprintln!(
        "  --expected-threshold <n> acts as the default threshold for manifest slots that omit `threshold`"
    );
    eprintln!(
        "Known treasury buckets: {}/{}/{}",
        MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD,
        MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
        MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE
    );
}

#[cfg(test)]
#[path = "oasis7_governance_registry_audit/tests.rs"]
mod tests;

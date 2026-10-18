use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};
use std::process;

use oasis7::viewer::ExclusiveDirectoryProcessLock;
use oasis7_runtime::{
    GovernanceFinalitySignerRegistry, GovernanceMainTokenControllerRegistry,
    GovernanceThresholdSignerPolicy, MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
    MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE, MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD,
    ReleaseSecurityPolicy, RollbackAuthorityRecord, RollbackAuthorityRegistry,
    RollbackAuthorityRole, World, WorldState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DEFAULT_FINALITY_SLOT_ID: &str = "governance.finality.v1";
const DEFAULT_GENESIS_CONTROLLER_ACCOUNT_ID: &str = "msig.genesis.v1";
const DEFAULT_CONTROLLER_THRESHOLD: u16 = 2;
const DEFAULT_STAKING_CONTROLLER_ACCOUNT_ID: &str = "msig.staking_governance.v1";
const DEFAULT_ECOSYSTEM_CONTROLLER_ACCOUNT_ID: &str = "msig.ecosystem_governance.v1";
const DEFAULT_SECURITY_CONTROLLER_ACCOUNT_ID: &str = "msig.security_council.v1";
const ROLLBACK_ON_CALL_SLOT_ID: &str = "ops.rollback.on_call.v1";
const ROLLBACK_GOVERNANCE_SLOT_ID: &str = "governance.rollback.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    world_dir: PathBuf,
    public_manifest: PathBuf,
    finality_slot_id: String,
    default_threshold: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct PublicManifestEntry {
    slot_id: String,
    #[serde(default)]
    signer_id: Option<String>,
    #[serde(default)]
    node_id: Option<String>,
    scheme: String,
    public_key_hex: String,
    #[serde(default)]
    stake: Option<u64>,
    #[serde(default)]
    threshold: Option<u16>,
    #[serde(default)]
    oc_account_id: String,
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
struct ManifestSlotThresholds {
    thresholds: BTreeMap<String, u16>,
}

#[derive(Debug, Serialize)]
struct StateRootProjection<'a> {
    state: &'a WorldState,
    manifest_hash: &'a str,
    policy_hash: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ImportSummary {
    world_dir: String,
    public_manifest: String,
    imported_finality_slot_id: String,
    finality_signer_count: usize,
    controller_policy_count: usize,
    rollback_authority_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    reconciled_latest_tick: Option<u64>,
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

    let summary = match run_import(options) {
        Ok(summary) => summary,
        Err(err) => {
            eprintln!("oasis7_governance_registry_import failed: {err}");
            process::exit(1);
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).expect("encode import summary")
    );
}

fn run_import(options: CliOptions) -> Result<ImportSummary, String> {
    let _world_writer_lock =
        ExclusiveDirectoryProcessLock::try_acquire(options.world_dir.as_path())
            .map_err(|err| format!("acquire world writer lock failed: {err}"))?;
    let manifest_bytes = std::fs::read(options.public_manifest.as_path()).map_err(|err| {
        format!(
            "read public manifest {} failed: {err}",
            options.public_manifest.display()
        )
    })?;
    let manifest: PublicManifest =
        serde_json::from_slice(manifest_bytes.as_slice()).map_err(|err| {
            format!(
                "decode public manifest {} failed: {err}",
                options.public_manifest.display()
            )
        })?;
    let entries = manifest.into_entries();
    let slot_thresholds =
        resolve_manifest_slot_thresholds(entries.as_slice(), options.default_threshold)?;
    let mut world = load_or_create_world(options.world_dir.as_path())?;
    let finality_registry = build_finality_registry(
        entries.as_slice(),
        options.finality_slot_id.as_str(),
        &slot_thresholds,
    )?;
    let controller_registry = build_main_token_controller_registry(
        entries.as_slice(),
        options.finality_slot_id.as_str(),
        &slot_thresholds,
    )?;
    let rollback_registry = build_rollback_authority_registry(entries.as_slice())?;
    world
        .set_governance_finality_signer_registry(finality_registry.clone())
        .map_err(|err| format!("write finality registry failed: {err:?}"))?;
    world
        .set_governance_main_token_controller_registry(controller_registry.clone())
        .map_err(|err| format!("write controller registry failed: {err:?}"))?;
    world
        .set_rollback_authority_registry(rollback_registry.clone())
        .map_err(|err| format!("write rollback authority registry failed: {err:?}"))?;
    let (world, reconciled_latest_tick) = reconcile_latest_tick_consensus_state_root(world)?;
    world
        .save_to_dir(options.world_dir.as_path())
        .map_err(|err| format!("save world {} failed: {err:?}", options.world_dir.display()))?;
    Ok(ImportSummary {
        world_dir: options.world_dir.display().to_string(),
        public_manifest: options.public_manifest.display().to_string(),
        imported_finality_slot_id: finality_registry.slot_id,
        finality_signer_count: finality_registry.signer_bindings.len(),
        controller_policy_count: controller_registry.controller_signer_policies.len(),
        rollback_authority_count: rollback_registry.len(),
        reconciled_latest_tick,
    })
}

fn build_rollback_authority_registry(
    entries: &[PublicManifestEntry],
) -> Result<RollbackAuthorityRegistry, String> {
    let build_record = |slot_id: &str, role: RollbackAuthorityRole| {
        let matching = entries
            .iter()
            .filter(|entry| entry.slot_id.trim() == slot_id)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(format!(
                "public manifest requires exactly one rollback authority entry slot_id={slot_id} actual={}",
                matching.len()
            ));
        }
        let entry = matching[0];
        validate_manifest_entry(entry)?;
        if entry.threshold != Some(1) {
            return Err(format!(
                "rollback authority slot requires explicit threshold=1 slot_id={slot_id}"
            ));
        }
        let authority_id = entry
            .signer_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("rollback authority slot requires signer_id slot_id={slot_id}")
            })?;
        Ok(RollbackAuthorityRecord {
            authority_id: authority_id.to_string(),
            role,
            public_key_hex: entry.public_key_hex.trim().to_ascii_lowercase(),
            active: true,
        })
    };
    RollbackAuthorityRegistry::new([
        build_record(ROLLBACK_ON_CALL_SLOT_ID, RollbackAuthorityRole::OnCall)?,
        build_record(
            ROLLBACK_GOVERNANCE_SLOT_ID,
            RollbackAuthorityRole::Governance,
        )?,
    ])
    .map_err(|err| format!("invalid rollback authority registry: {err:?}"))
}

fn reconcile_latest_tick_consensus_state_root(
    world: World,
) -> Result<(World, Option<u64>), String> {
    let manifest_hash = world
        .current_manifest_hash()
        .map_err(|err| format!("compute current manifest hash failed: {err:?}"))?;
    let policy_hash = hash_json(&world.policies())
        .map_err(|err| format!("compute current policy hash failed: {err:?}"))?;
    let state_root = hash_json(&StateRootProjection {
        state: world.state(),
        manifest_hash: manifest_hash.as_str(),
        policy_hash: policy_hash.as_str(),
    })
    .map_err(|err| format!("compute current state root failed: {err:?}"))?;
    let mut snapshot = world.snapshot();
    let Some(record) = snapshot.tick_consensus_records.last_mut() else {
        return Ok((world, None));
    };
    if record.block.header.state_root == state_root
        && record.block.execution_digest.state_projection_hash == state_root
    {
        return Ok((world, None));
    }
    let reconciled_tick = record.block.header.tick;
    record.block.header.state_root = state_root.clone();
    record.block.execution_digest.state_projection_hash = state_root;
    record.certificate.block_hash = record.block.block_hash();
    let reconciled = oasis7::runtime_host::world::from_snapshot(snapshot, world.journal().clone())
        .map_err(|err| format!("rebuild world after state-root reconciliation failed: {err:?}"))?;
    Ok((reconciled, Some(reconciled_tick)))
}

fn load_or_create_world(world_dir: &Path) -> Result<World, String> {
    let snapshot_path = world_dir.join("snapshot.json");
    let journal_path = world_dir.join("journal.json");
    if !snapshot_path.exists() || !journal_path.exists() {
        return Ok(oasis7::runtime_host::world::new_production_hardened());
    }
    oasis7::runtime_host::world::load_from_dir(world_dir)
        .map(|world| {
            world.with_release_security_policy(ReleaseSecurityPolicy::production_hardened())
        })
        .map_err(|err| format!("load world {} failed: {err:?}", world_dir.display()))
}

fn build_finality_registry(
    entries: &[PublicManifestEntry],
    finality_slot_id: &str,
    slot_thresholds: &ManifestSlotThresholds,
) -> Result<GovernanceFinalitySignerRegistry, String> {
    let mut signer_bindings = BTreeMap::new();
    let mut validator_stakes = BTreeMap::new();
    for entry in entries
        .iter()
        .filter(|entry| entry.slot_id == finality_slot_id)
    {
        validate_manifest_entry(entry)?;
        let validator_id = entry_validator_id(entry)?;
        let stake = entry.stake.unwrap_or(100);
        if stake == 0 {
            return Err(format!(
                "finality manifest entry stake must be > 0 slot_id={finality_slot_id} validator_id={validator_id}"
            ));
        }
        let binding_key = format!("{finality_slot_id}.{validator_id}");
        signer_bindings.insert(binding_key.clone(), entry.public_key_hex.trim().to_string());
        validator_stakes.insert(binding_key, stake);
    }
    if signer_bindings.is_empty() {
        return Err(format!(
            "public manifest does not contain finality slot {finality_slot_id}"
        ));
    }
    let threshold = slot_thresholds
        .thresholds
        .get(finality_slot_id)
        .copied()
        .ok_or_else(|| format!("missing threshold for finality slot {finality_slot_id}"))?;
    Ok(GovernanceFinalitySignerRegistry {
        slot_id: finality_slot_id.to_string(),
        threshold,
        threshold_bps: 0,
        signer_bindings,
        validator_stakes,
    })
}

fn build_main_token_controller_registry(
    entries: &[PublicManifestEntry],
    finality_slot_id: &str,
    slot_thresholds: &ManifestSlotThresholds,
) -> Result<GovernanceMainTokenControllerRegistry, String> {
    let mut controller_signer_policies = BTreeMap::new();
    for entry in entries.iter().filter(|entry| {
        entry.slot_id != finality_slot_id
            && entry.slot_id.trim() != ROLLBACK_ON_CALL_SLOT_ID
            && entry.slot_id.trim() != ROLLBACK_GOVERNANCE_SLOT_ID
    }) {
        validate_manifest_entry(entry)?;
        let slot_id = entry.slot_id.trim().to_string();
        let threshold = slot_thresholds
            .thresholds
            .get(slot_id.as_str())
            .copied()
            .ok_or_else(|| format!("missing threshold for controller slot {}", entry.slot_id))?;
        controller_signer_policies
            .entry(slot_id)
            .or_insert_with(|| GovernanceThresholdSignerPolicy {
                threshold,
                allowed_public_keys: BTreeSet::new(),
            })
            .allowed_public_keys
            .insert(entry.public_key_hex.trim().to_string());
    }
    if controller_signer_policies.is_empty() {
        return Err("public manifest does not contain any controller signer slots".to_string());
    }
    Ok(GovernanceMainTokenControllerRegistry {
        genesis_controller_account_id: DEFAULT_GENESIS_CONTROLLER_ACCOUNT_ID.to_string(),
        treasury_bucket_controller_slots: default_treasury_bucket_controller_slots(),
        restricted_starter_claim_admin_account_ids: BTreeSet::new(),
        controller_signer_policies,
    })
}

fn default_treasury_bucket_controller_slots() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD.to_string(),
            DEFAULT_STAKING_CONTROLLER_ACCOUNT_ID.to_string(),
        ),
        (
            MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.to_string(),
            DEFAULT_ECOSYSTEM_CONTROLLER_ACCOUNT_ID.to_string(),
        ),
        (
            MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE.to_string(),
            DEFAULT_SECURITY_CONTROLLER_ACCOUNT_ID.to_string(),
        ),
    ])
}

fn resolve_manifest_slot_thresholds(
    entries: &[PublicManifestEntry],
    default_threshold: u16,
) -> Result<ManifestSlotThresholds, String> {
    let mut thresholds = BTreeMap::new();
    for entry in entries {
        validate_manifest_entry(entry)?;
        let slot_id = entry.slot_id.trim();
        let threshold = entry.threshold.unwrap_or(default_threshold);
        if threshold == 0 {
            let signer_label = entry
                .signer_id
                .as_deref()
                .or(entry.node_id.as_deref())
                .unwrap_or("");
            return Err(format!(
                "manifest threshold must be > 0 slot_id={} signer_id={}",
                entry.slot_id, signer_label
            ));
        }
        match thresholds.get(slot_id) {
            Some(existing) if *existing != threshold => {
                return Err(format!(
                    "manifest slot threshold mismatch slot_id={} expected={} actual={threshold}",
                    entry.slot_id, existing
                ));
            }
            Some(_) => {}
            None => {
                thresholds.insert(slot_id.to_string(), threshold);
            }
        }
    }
    Ok(ManifestSlotThresholds { thresholds })
}

fn validate_manifest_entry(entry: &PublicManifestEntry) -> Result<(), String> {
    let signer_label = entry
        .signer_id
        .as_deref()
        .or(entry.node_id.as_deref())
        .unwrap_or("");
    if !entry.scheme.trim().eq_ignore_ascii_case("ed25519") {
        return Err(format!(
            "unsupported signer scheme slot_id={} signer_id={} scheme={}",
            entry.slot_id, signer_label, entry.scheme
        ));
    }
    let slot_id = entry.slot_id.trim();
    let public_key_hex = entry.public_key_hex.trim();
    if slot_id.is_empty() || public_key_hex.is_empty() {
        return Err("manifest entry slot_id/public_key_hex cannot be empty".to_string());
    }
    if signer_label.trim().is_empty() {
        return Err(format!(
            "manifest entry requires signer_id or node_id slot_id={}",
            entry.slot_id
        ));
    }
    if public_key_hex.len() != 64 || !public_key_hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "manifest entry public_key_hex must be 32-byte hex slot_id={} signer_id={}",
            entry.slot_id, signer_label
        ));
    }
    if entry.threshold.is_some_and(|value| value == 0) {
        return Err(format!(
            "manifest entry threshold must be > 0 slot_id={} signer_id={}",
            entry.slot_id, signer_label
        ));
    }
    if entry.stake.is_some_and(|value| value == 0) {
        return Err(format!(
            "manifest entry stake must be > 0 slot_id={} signer_id={}",
            entry.slot_id, signer_label
        ));
    }
    Ok(())
}

fn entry_validator_id(entry: &PublicManifestEntry) -> Result<String, String> {
    entry
        .node_id
        .as_deref()
        .or(entry.signer_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            format!(
                "manifest entry requires signer_id or node_id slot_id={}",
                entry.slot_id
            )
        })
}

fn hash_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(value)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn parse_options<'a>(args: impl Iterator<Item = &'a str>) -> Result<CliOptions, String> {
    let mut world_dir: Option<PathBuf> = None;
    let mut public_manifest: Option<PathBuf> = None;
    let mut finality_slot_id = DEFAULT_FINALITY_SLOT_ID.to_string();
    let mut default_threshold = DEFAULT_CONTROLLER_THRESHOLD;
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
            "--controller-threshold" => {
                default_threshold = parse_required_value(&mut iter, "--controller-threshold")?
                    .parse::<u16>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        "--controller-threshold requires a positive integer".to_string()
                    })?;
            }
            _ => return Err(format!("unknown option: {arg}")),
        }
    }

    let world_dir = world_dir.ok_or_else(|| "--world-dir is required".to_string())?;
    let public_manifest =
        public_manifest.ok_or_else(|| "--public-manifest is required".to_string())?;
    Ok(CliOptions {
        world_dir,
        public_manifest,
        finality_slot_id,
        default_threshold,
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
        "Usage: oasis7_governance_registry_import --world-dir <dir> --public-manifest <file> [--finality-slot-id <slot>] [--controller-threshold <n>]"
    );
    eprintln!(
        "  --controller-threshold <n> acts as the default threshold for manifest entries that omit `threshold`"
    );
}

#[cfg(test)]
mod tests {
    use super::{parse_options, run_import};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        std::env::temp_dir().join(format!("oasis7-governance-import-{prefix}-{unique}"))
    }

    fn governed_fixture(mut value: serde_json::Value) -> serde_json::Value {
        let entries = if let Some(entries) = value.as_array_mut() {
            entries
        } else {
            value["entries"].as_array_mut().expect("manifest entries")
        };
        entries.extend([
            serde_json::json!({
                "slot_id": "ops.rollback.on_call.v1", "signer_id": "fixture-rollback-on-call",
                "scheme": "ed25519", "threshold": 1,
                "public_key_hex": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            }),
            serde_json::json!({
                "slot_id": "governance.rollback.v1", "signer_id": "fixture-rollback-governance",
                "scheme": "ed25519", "threshold": 1,
                "public_key_hex": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
            }),
        ]);
        value
    }

    #[test]
    fn parse_options_accepts_required_flags() {
        let options = parse_options(
            [
                "--world-dir",
                "output/world",
                "--public-manifest",
                "manifest.json",
            ]
            .into_iter(),
        )
        .expect("parse options");
        assert_eq!(options.finality_slot_id, "governance.finality.v1");
        assert_eq!(options.default_threshold, 2);
    }

    #[test]
    fn import_writes_governance_registries_into_world() {
        let temp_dir = temp_dir("import");
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let manifest_path = temp_dir.join("public_manifest.json");
        std::fs::write(
            manifest_path.as_path(),
            serde_json::to_vec_pretty(&governed_fixture(serde_json::json!([
                serde_json::json!({
                    "slot_id": "governance.finality.v1",
                    "node_id": "validator-a",
                    "scheme": "ed25519",
                    "public_key_hex": "54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc",
                    "stake": 70
                }),
                serde_json::json!({
                    "slot_id": "governance.finality.v1",
                    "node_id": "validator-b",
                    "scheme": "ed25519",
                    "public_key_hex": "38dac17ff403cc19de033e47be7cf7b5354635fbc5c1976d7c532e20494aace4",
                    "stake": 20
                }),
                serde_json::json!({
                    "slot_id": "governance.finality.v1",
                    "node_id": "validator-c",
                    "scheme": "ed25519",
                    "public_key_hex": "e22bd5029176296712fb1a477f91c15775e5ab858181cb4172839ced526f12c8",
                    "stake": 10
                }),
                serde_json::json!({
                    "slot_id": "msig.genesis.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                }),
                serde_json::json!({
                    "slot_id": "msig.genesis.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "7014e88a6336ec91fc7e6ffb044b50232e4411ec403f90123fa8a202a3420a04"
                }),
                serde_json::json!({
                    "slot_id": "msig.staking_governance.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                }),
                serde_json::json!({
                    "slot_id": "msig.staking_governance.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb"
                }),
                serde_json::json!({
                    "slot_id": "msig.ecosystem_governance.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "0241f2e23305407676f2a5cec6d154da74944b2a366b2b2b6913cb746d402d0e"
                }),
                serde_json::json!({
                    "slot_id": "msig.ecosystem_governance.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "960137cd5d675a517daed5f14ea6bea460e196fda4310a581ecd448f3bcd20b4"
                }),
                serde_json::json!({
                    "slot_id": "msig.security_council.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "d09de9413371ae42f643e4f8f31e2139611d1617809375b1ad884df3fb089448"
                }),
                serde_json::json!({
                    "slot_id": "msig.security_council.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "aa738a832b0d3bf371d231a0bd8502fd411f2a9723246e5d7d215e8fb0ecbb7c"
                })
            ])))
            .expect("encode manifest"),
        )
        .expect("write manifest");

        let summary = run_import(super::CliOptions {
            world_dir: temp_dir.join("world"),
            public_manifest: manifest_path,
            finality_slot_id: "governance.finality.v1".to_string(),
            default_threshold: 2,
        })
        .expect("run import");
        assert_eq!(summary.finality_signer_count, 3);

        let world =
            oasis7::runtime_host::world::load_from_dir(temp_dir.join("world")).expect("load world");
        let finality_registry = world
            .governance_finality_signer_registry()
            .expect("finality registry");
        assert_eq!(
            finality_registry
                .validator_stakes
                .get("governance.finality.v1.validator-a"),
            Some(&70)
        );
        assert_eq!(
            finality_registry
                .validator_stakes
                .get("governance.finality.v1.validator-b"),
            Some(&20)
        );
        assert_eq!(
            finality_registry
                .validator_stakes
                .get("governance.finality.v1.validator-c"),
            Some(&10)
        );
        assert!(world.governance_main_token_controller_registry().is_some());
    }

    #[test]
    fn import_accepts_document_manifest_entries() {
        let temp_dir = temp_dir("document-manifest");
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let manifest_path = temp_dir.join("public_manifest.json");
        std::fs::write(
            manifest_path.as_path(),
            serde_json::to_vec_pretty(&governed_fixture(serde_json::json!({
                "schema_version": "test",
                "entries": [
                    {
                        "slot_id": "governance.finality.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc"
                    },
                    {
                        "slot_id": "msig.genesis.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                    },
                    {
                        "slot_id": "msig.staking_governance.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                    },
                    {
                        "slot_id": "msig.ecosystem_governance.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "0241f2e23305407676f2a5cec6d154da74944b2a366b2b2b6913cb746d402d0e"
                    },
                    {
                        "slot_id": "msig.security_council.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "d09de9413371ae42f643e4f8f31e2139611d1617809375b1ad884df3fb089448"
                    }
                ]
            })))
            .expect("encode manifest"),
        )
        .expect("write manifest");

        let summary = run_import(super::CliOptions {
            world_dir: temp_dir.join("world"),
            public_manifest: manifest_path,
            finality_slot_id: "governance.finality.v1".to_string(),
            default_threshold: 1,
        })
        .expect("run import");

        assert_eq!(summary.finality_signer_count, 1);
        assert_eq!(summary.controller_policy_count, 4);
    }

    #[test]
    fn import_uses_manifest_specific_threshold_for_liveops_slot() {
        let temp_dir = temp_dir("liveops-threshold");
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let manifest_path = temp_dir.join("public_manifest.json");
        std::fs::write(
            manifest_path.as_path(),
            serde_json::to_vec_pretty(&governed_fixture(serde_json::json!([
                serde_json::json!({
                    "slot_id": "governance.finality.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc"
                }),
                serde_json::json!({
                    "slot_id": "governance.finality.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "38dac17ff403cc19de033e47be7cf7b5354635fbc5c1976d7c532e20494aace4"
                }),
                serde_json::json!({
                    "slot_id": "governance.finality.v1",
                    "signer_id": "signer03",
                    "scheme": "ed25519",
                    "public_key_hex": "e22bd5029176296712fb1a477f91c15775e5ab858181cb4172839ced526f12c8"
                }),
                serde_json::json!({
                    "slot_id": "msig.genesis.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                }),
                serde_json::json!({
                    "slot_id": "msig.genesis.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "7014e88a6336ec91fc7e6ffb044b50232e4411ec403f90123fa8a202a3420a04"
                }),
                serde_json::json!({
                    "slot_id": "msig.staking_governance.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                }),
                serde_json::json!({
                    "slot_id": "msig.staking_governance.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb"
                }),
                serde_json::json!({
                    "slot_id": "msig.ecosystem_governance.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "0241f2e23305407676f2a5cec6d154da74944b2a366b2b2b6913cb746d402d0e"
                }),
                serde_json::json!({
                    "slot_id": "msig.ecosystem_governance.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "960137cd5d675a517daed5f14ea6bea460e196fda4310a581ecd448f3bcd20b4"
                }),
                serde_json::json!({
                    "slot_id": "msig.security_council.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "d09de9413371ae42f643e4f8f31e2139611d1617809375b1ad884df3fb089448"
                }),
                serde_json::json!({
                    "slot_id": "msig.security_council.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "aa738a832b0d3bf371d231a0bd8502fd411f2a9723246e5d7d215e8fb0ecbb7c"
                }),
                serde_json::json!({
                    "slot_id": "liveops",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "threshold": 1,
                    "public_key_hex": "14699ee340994e43103490585a96671ec66a3280bc0f90518f29cd1866f0fa7d"
                }),
                serde_json::json!({
                    "slot_id": "liveops",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "threshold": 1,
                    "public_key_hex": "b6517819f923b8b25989042b03e00854673b5517be88e3f568141373105ca77f"
                })
            ])))
            .expect("encode manifest"),
        )
        .expect("write manifest");

        run_import(super::CliOptions {
            world_dir: temp_dir.join("world"),
            public_manifest: manifest_path,
            finality_slot_id: "governance.finality.v1".to_string(),
            default_threshold: 2,
        })
        .expect("run import");

        let world =
            oasis7::runtime_host::world::load_from_dir(temp_dir.join("world")).expect("load world");
        let registry = world
            .governance_main_token_controller_registry()
            .expect("controller registry");
        assert_eq!(
            registry
                .controller_signer_policies
                .get("liveops")
                .expect("liveops policy")
                .threshold,
            1
        );
        assert_eq!(
            registry
                .controller_signer_policies
                .get("msig.genesis.v1")
                .expect("genesis policy")
                .threshold,
            2
        );
    }

    #[test]
    fn import_rejects_manifest_when_slot_thresholds_disagree() {
        let temp_dir = temp_dir("threshold-mismatch");
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let manifest_path = temp_dir.join("public_manifest.json");
        std::fs::write(
            manifest_path.as_path(),
            serde_json::to_vec_pretty(&governed_fixture(serde_json::json!([
                serde_json::json!({
                    "slot_id": "governance.finality.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc"
                }),
                serde_json::json!({
                    "slot_id": "liveops",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "threshold": 1,
                    "public_key_hex": "14699ee340994e43103490585a96671ec66a3280bc0f90518f29cd1866f0fa7d"
                }),
                serde_json::json!({
                    "slot_id": "liveops",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "threshold": 2,
                    "public_key_hex": "b6517819f923b8b25989042b03e00854673b5517be88e3f568141373105ca77f"
                })
            ])))
            .expect("encode manifest"),
        )
        .expect("write manifest");

        let err = run_import(super::CliOptions {
            world_dir: temp_dir.join("world"),
            public_manifest: manifest_path,
            finality_slot_id: "governance.finality.v1".to_string(),
            default_threshold: 2,
        })
        .expect_err("manifest threshold mismatch must fail");
        assert!(err.contains("manifest slot threshold mismatch"), "{err}");
    }

    #[test]
    fn import_reconciles_latest_tick_consensus_state_root_for_existing_world() {
        let temp_dir = temp_dir("reconcile-existing-world");
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let manifest_path = temp_dir.join("public_manifest.json");
        std::fs::write(
            manifest_path.as_path(),
            serde_json::to_vec_pretty(&governed_fixture(serde_json::json!([
                serde_json::json!({
                    "slot_id": "governance.finality.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc"
                }),
                serde_json::json!({
                    "slot_id": "governance.finality.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "38dac17ff403cc19de033e47be7cf7b5354635fbc5c1976d7c532e20494aace4"
                }),
                serde_json::json!({
                    "slot_id": "governance.finality.v1",
                    "signer_id": "signer03",
                    "scheme": "ed25519",
                    "public_key_hex": "e22bd5029176296712fb1a477f91c15775e5ab858181cb4172839ced526f12c8"
                }),
                serde_json::json!({
                    "slot_id": "msig.genesis.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                }),
                serde_json::json!({
                    "slot_id": "msig.genesis.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "7014e88a6336ec91fc7e6ffb044b50232e4411ec403f90123fa8a202a3420a04"
                }),
                serde_json::json!({
                    "slot_id": "msig.staking_governance.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                }),
                serde_json::json!({
                    "slot_id": "msig.staking_governance.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb"
                }),
                serde_json::json!({
                    "slot_id": "msig.ecosystem_governance.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "0241f2e23305407676f2a5cec6d154da74944b2a366b2b2b6913cb746d402d0e"
                }),
                serde_json::json!({
                    "slot_id": "msig.ecosystem_governance.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "960137cd5d675a517daed5f14ea6bea460e196fda4310a581ecd448f3bcd20b4"
                }),
                serde_json::json!({
                    "slot_id": "msig.security_council.v1",
                    "signer_id": "signer01",
                    "scheme": "ed25519",
                    "public_key_hex": "d09de9413371ae42f643e4f8f31e2139611d1617809375b1ad884df3fb089448"
                }),
                serde_json::json!({
                    "slot_id": "msig.security_council.v1",
                    "signer_id": "signer02",
                    "scheme": "ed25519",
                    "public_key_hex": "aa738a832b0d3bf371d231a0bd8502fd411f2a9723246e5d7d215e8fb0ecbb7c"
                })
            ])))
            .expect("encode manifest"),
        )
        .expect("write manifest");

        let world_dir = temp_dir.join("world");
        let mut world = oasis7::runtime_host::world::new_production_hardened();
        world.step().expect("step 1");
        world.step().expect("step 2");
        world.save_to_dir(world_dir.as_path()).expect("seed world");

        let summary = run_import(super::CliOptions {
            world_dir: world_dir.clone(),
            public_manifest: manifest_path,
            finality_slot_id: "governance.finality.v1".to_string(),
            default_threshold: 2,
        })
        .expect("run import");
        assert!(summary.reconciled_latest_tick.is_some());

        let loaded =
            oasis7::runtime_host::world::load_from_dir(world_dir).expect("load reconciled world");
        assert!(loaded.governance_finality_signer_registry().is_some());
        assert!(loaded.governance_main_token_controller_registry().is_some());
    }

    #[test]
    fn load_or_create_world_hardens_release_policy_for_new_and_existing_worlds() {
        let root = temp_dir("release-policy");
        std::fs::create_dir_all(&root).expect("create temp dir");
        let world_dir = root.join("world");

        let created = super::load_or_create_world(world_dir.as_path()).expect("create world");
        assert!(created.release_security_policy().is_production_hardened());

        let legacy_dir = root.join("legacy-world");
        let legacy = oasis7::runtime_host::world::new();
        legacy
            .save_to_dir(legacy_dir.as_path())
            .expect("save legacy world");

        let loaded = super::load_or_create_world(legacy_dir.as_path()).expect("load world");
        assert!(loaded.release_security_policy().is_production_hardened());
    }
}

#[cfg(test)]
#[path = "oasis7_governance_registry_import/rollback_tests.rs"]
mod rollback_authority_tests;

#[cfg(test)]
#[path = "oasis7_governance_registry_import/world_writer_lock_tests.rs"]
mod world_writer_lock_tests;

//! Offline DevLocal authorization assembly; never saves a World.
use oasis7_agent_api::game::{DecimalU64, GameIdentityNamespaceV1};
use oasis7_client_api::world_service::{
    CommitRef, SignedReadRequest, WorldIdentity, participant_identity_snapshot::IdentitySnapshot,
    sign_read_request,
};
use oasis7_runtime::{ChainResourceDerivationContext, DevLocalParticipantAuthority, World};
use oasis7_world_state::service_admission::{participant::*, participant_policy::*};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[cfg(test)]
#[path = "devlocal_participant_cli_copy.rs"]
mod copy;
#[path = "devlocal_participant_cli_files.rs"]
mod files;
#[cfg(test)]
#[path = "devlocal_participant_cli_tests.rs"]
mod tests;

#[cfg(not(test))]
use super::execution_bridge::world_service_read;
#[cfg(test)]
use super::execution_bridge_real_tests::real_execution_bridge::world_service_read;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    world: WorldIdentity,
    world_directory: PathBuf,
    origin_root: PathBuf,
    execution_records_directory: PathBuf,
    storage_root: PathBuf,
    expected_head: CommitRef,
    node_id: String,
    node_key_directory: PathBuf,
    service_public_key: String,
    operator_public_key: String,
    identity_namespace: GameIdentityNamespaceV1,
    world_identity_scope: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InspectConfig {
    world: WorldIdentity,
    world_directory: PathBuf,
    origin_root: PathBuf,
    execution_records_directory: PathBuf,
    storage_root: PathBuf,
    node_id: String,
    node_key_directory: PathBuf,
    service_public_key: String,
    operator_public_key: String,
    identity_namespace: GameIdentityNamespaceV1,
    world_identity_scope: String,
}

impl From<&Config> for InspectConfig {
    fn from(config: &Config) -> Self {
        Self {
            world: config.world.clone(),
            world_directory: config.world_directory.clone(),
            origin_root: config.origin_root.clone(),
            execution_records_directory: config.execution_records_directory.clone(),
            storage_root: config.storage_root.clone(),
            node_id: config.node_id.clone(),
            node_key_directory: config.node_key_directory.clone(),
            service_public_key: config.service_public_key.clone(),
            operator_public_key: config.operator_public_key.clone(),
            identity_namespace: config.identity_namespace.clone(),
            world_identity_scope: config.world_identity_scope.clone(),
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepared {
    schema_revision: u32,
    canonical_commit: CommitRef,
    world: WorldIdentity,
    branch_id: String,
    reorg_epoch: u64,
    registry_digest: String,
    legacy_owner_state_digest: String,
    snapshot_digest: String,
    resource_counts: ResourceCounts,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceCounts {
    agents: usize,
    tasks: usize,
    goals: usize,
    grants: usize,
}

const HELP: &str = "devlocal-participant inspect-head --config <private-source-config> --output <new-private-commit> | prepare --config <private-config> --output <new-private-prestate> | finalize --config <private-config> --prepared <private-prestate> --identity-snapshot <consistent-export> --policy <private-policy> --entries <owner-signed-entries> --issuer-key <private-raw32> --operator-key <private-raw32> --output <new-private-authorization>\nStop world and identity writers and retain their backups. Reads and validates the original world under its writer lock; only publishes validated head or authorization material. Installation uses existing DevLocal startup flags and normal canonical commit. No governance/finality or Runtime acceptance is created.";

pub(super) fn run<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let command = args.next().ok_or(HELP)?;
    let allowed: &[&str] = match command {
        "prepare" | "inspect-head" => &["--config", "--output"],
        "finalize" => &[
            "--config",
            "--prepared",
            "--identity-snapshot",
            "--policy",
            "--entries",
            "--issuer-key",
            "--operator-key",
            "--output",
        ],
        _ => return Err(HELP.into()),
    };
    let args: Vec<_> = args.collect();
    if args.len() != allowed.len() * 2 {
        return Err(HELP.into());
    }
    let mut options = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        if !allowed.contains(&pair[0]) || options.insert(pair[0], pair[1]).is_some() {
            return Err(HELP.into());
        }
    }
    if command == "inspect-head" {
        let config: InspectConfig = files::read(Path::new(options["--config"]))?;
        validate_source(&config)?;
        validate_output(&config, Path::new(options["--output"]))?;
        let _lock =
            super::world_writer_lock::acquire_live_world_writer_lock(&config.world_directory)?;
        let (_, commit) = load_world(&config, None)?;
        files::publish(Path::new(options["--output"]), &commit)?;
        println!(
            "{{\"state\":\"canonical_head_validated\",\"world_written\":false,\"authorization_created\":false}}"
        );
        return Ok(());
    }
    let config: Config = files::read(Path::new(options["--config"]))?;
    validate_config(&config)?;
    let source = InspectConfig::from(&config);
    validate_output(&source, Path::new(options["--output"]))?;
    // Same exclusion mechanism as the live execution host. It cannot inspect a
    // running writer and then race a second writer before taking its snapshot.
    let _lock = super::world_writer_lock::acquire_live_world_writer_lock(&config.world_directory)?;
    let (mut world, canonical_commit) = load_world(&source, Some(&config.expected_head))?;
    let prepared = prepare(&world, &config.world, &canonical_commit)?;
    let output = Path::new(options["--output"]);
    if command == "prepare" {
        files::publish(output, &prepared)?;
    } else {
        let expected: Prepared = files::read(Path::new(options["--prepared"]))?;
        if prepared != expected {
            return Err(
                "world changed since prepare; retain originals and collect fresh authorization"
                    .into(),
            );
        }
        let snapshot: IdentitySnapshot = files::read(Path::new(options["--identity-snapshot"]))?;
        let policy: ParticipantEntryPolicyV1 = files::read(Path::new(options["--policy"]))?;
        let entries: Vec<SignedReadRequest<ParticipantMigrationBindingV1>> =
            files::read(Path::new(options["--entries"]))?;
        let issuer = oasis7_game_client::private_key_file::read_private_key_file(Path::new(
            options["--issuer-key"],
        ))?;
        let operator = oasis7_game_client::private_key_file::read_private_key_file(Path::new(
            options["--operator-key"],
        ))?;
        let authorization = assemble(
            &mut world, &config, &prepared, &snapshot, policy, entries, &issuer, &operator,
        )?;
        files::publish(output, &authorization)?;
    }
    println!(
        "{{\"state\":\"validated_material_published\",\"world_written\":false,\"canonical_install_required\":true}}"
    );
    Ok(())
}

fn validate_output(config: &InspectConfig, output: &Path) -> Result<(), String> {
    let parent = output.parent().ok_or("offline output parent unavailable")?;
    files::canonical(parent)?;
    let name = output
        .file_name()
        .ok_or("offline output filename unavailable")?;
    let resolved = parent.join(name);
    if !output.is_absolute() || resolved != output {
        return Err("offline output must have a canonical parent and filename".into());
    }
    if [
        &config.origin_root,
        &config.world_directory,
        &config.execution_records_directory,
        &config.storage_root,
        &config.node_key_directory,
    ]
    .into_iter()
    .any(|source| resolved.starts_with(source))
    {
        return Err(
            "offline output must remain outside original world, history and identity sources"
                .into(),
        );
    }
    Ok(())
}

fn validate_config(config: &Config) -> Result<(), String> {
    validate_source(&InspectConfig::from(config))?;
    config.expected_head.validate().map_err(|e| e.to_string())?;
    if config.expected_head.world != config.world {
        return Err("trusted canonical head world differs from configured world".into());
    }
    Ok(())
}

fn validate_source(config: &InspectConfig) -> Result<(), String> {
    config.world.validate().map_err(|e| e.to_string())?;
    if config.node_id.trim().is_empty()
        || config.world_identity_scope.trim().is_empty()
        || config.identity_namespace.service_id.trim().is_empty()
        || config.identity_namespace.environment_id.trim().is_empty()
        || config.identity_namespace.instance_epoch.0 == 0
    {
        return Err("explicit world, identity namespace and scope required".into());
    }
    for path in [
        &config.world_directory,
        &config.origin_root,
        &config.node_key_directory,
        &config.execution_records_directory,
        &config.storage_root,
    ] {
        files::canonical(path)?;
    }
    if config.origin_root.join("world") != config.world_directory {
        return Err("origin must identify the actual existing execution world".into());
    }
    authority(&config.operator_public_key)?;
    Ok(())
}

fn authority(pin: &str) -> Result<DevLocalParticipantAuthority, String> {
    DevLocalParticipantAuthority::new(
        oasis7_proto::storage_profile::StorageProfile::DevLocal,
        &oasis7_runtime::ReleaseSecurityPolicy::default(),
        pin.into(),
    )
    .map_err(|e| format!("{e:?}"))
}

fn load_world(
    config: &InspectConfig,
    expected: Option<&CommitRef>,
) -> Result<(World, CommitRef), String> {
    let (keypair, _) = super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
        &config.node_key_directory,
    )?;
    let signer = super::build_feedback_submit_signer(&config.node_id, &keypair)?;
    if signer.public_key_hex != config.service_public_key {
        return Err("independently pinned service key differs from original node".into());
    }
    let origin = super::preview_world_init::verify_existing_origin(
        &config.origin_root,
        &config.world.world_id,
        &config.node_id,
        &config.service_public_key,
    )?;
    if origin != config.world {
        return Err("original signed world identity differs from configured world".into());
    }
    let auth = authority(&config.operator_public_key)?;
    let context = world_service_read::RestoreContext::new(
        oasis7_runtime::ReleaseSecurityPolicy::default(),
        Some(auth.clone()),
    )?;
    let pinned = world_service_read::pin_offline_current_with_context(
        &config.execution_records_directory,
        &config.storage_root,
        &config.world,
        expected,
        &context,
    )?;
    let canonical_commit = pinned.commit;
    let world = pinned.world;
    let m = world.chain_resource_manifest();
    if m.world_id != config.world.world_id
        || m.genesis_ref
            .as_ref()
            .is_some_and(|g| g != &config.world.genesis_digest)
    {
        return Err("current world identity differs from authenticated original".into());
    }
    if m.genesis_ref.is_some() {
        return Ok((world, canonical_commit));
    }
    // Detached derivation only, identical to the live host's verified-origin
    // path. Neither this snapshot nor the validation candidate is persisted.
    let snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: &m.world_id,
            chain_id: &m.chain_id,
            genesis_ref: Some(&config.world.genesis_digest),
            created_at_height: m.created_at_height,
            manifest_height: m.manifest_height,
            commit_block_hash: m.created_at_block_hash.as_deref(),
            tick: world.state().time,
        },
        m.world_config_hash.clone(),
        m.generation_algorithm_hash.clone(),
    );
    oasis7::runtime_host::world::from_snapshot_with_devlocal_participant_authority(
        snapshot,
        world.journal().clone(),
        auth,
    )
    .map(|world| (world, canonical_commit))
    .map_err(|e| format!("origin candidate rejected: {e:?}"))
}

fn prepare(
    world: &World,
    identity: &WorldIdentity,
    canonical_commit: &CommitRef,
) -> Result<Prepared, String> {
    if canonical_commit.world != *identity {
        return Err("canonical prepare world differs from trusted identity".into());
    }
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    let bytes =
        serde_cbor::to_vec(&world.snapshot()).map_err(|_| "world snapshot digest unavailable")?;
    let mut tasks = 0;
    let mut goals = 0;
    for agent in world.state().agents.keys() {
        if let Some(task) =
            oasis7_runtime::service_admission::game_task::current_control(world, agent)?
        {
            tasks += 1;
            goals += usize::from(task.latest_goal_revision.0 > 0);
        }
    }
    let resource_counts = ResourceCounts {
        agents: world.state().agents.len(),
        tasks,
        goals,
        grants: world
            .capability_revocation_state()
            .agent_signer_delegations
            .values()
            .filter(|g| !g.revoked)
            .count(),
    };
    Ok(Prepared {
        schema_revision: 2,
        canonical_commit: canonical_commit.clone(),
        world: identity.clone(),
        branch_id: binding.branch_id,
        reorg_epoch: binding.reorg_epoch,
        registry_digest: world.participant_registry_digest()?,
        legacy_owner_state_digest: world.participant_legacy_owner_state_digest()?,
        snapshot_digest: format!("blake3:{}", blake3::hash(&bytes)),
        resource_counts,
    })
}

#[allow(clippy::too_many_arguments)]
fn assemble(
    world: &mut World,
    config: &Config,
    prepared: &Prepared,
    snapshot: &IdentitySnapshot,
    policy: ParticipantEntryPolicyV1,
    entries: Vec<SignedReadRequest<ParticipantMigrationBindingV1>>,
    issuer: &[u8; 32],
    operator: &[u8; 32],
) -> Result<SignedReadRequest<DevLocalParticipantPolicyAuthorizationV1>, String> {
    snapshot.verify().map_err(|e| e.to_string())?;
    if prepared.schema_revision != 2 || prepared.canonical_commit != config.expected_head {
        return Err("prepared canonical commit differs from trusted current head".into());
    }
    let namespace = GameIdentityNamespaceV1 {
        service_id: snapshot.identity_namespace.identity_service_id.clone(),
        environment_id: snapshot.identity_namespace.environment_id.clone(),
        instance_epoch: DecimalU64(snapshot.rp_config.identity_instance_epoch),
    };
    let public = |seed: &[u8; 32]| {
        hex::encode(
            ed25519_dalek::SigningKey::from_bytes(seed)
                .verifying_key()
                .to_bytes(),
        )
    };
    if namespace != config.identity_namespace
        || policy.identity_namespace != namespace
        || policy.world_id != config.world.world_id
        || policy.genesis_digest != config.world.genesis_digest
        || prepared.world != config.world
        || policy.issuer_public_key != public(issuer)
        || config.operator_public_key != public(operator)
    {
        return Err(
            "snapshot, policy, issuer or operator differs from trusted configuration".into(),
        );
    }
    for entry in &entries {
        let account = &entry.request.registration.account_id;
        if !snapshot.accounts.iter().any(|a| &a.account_id == account)
            || !snapshot.bindings.iter().any(|b| {
                &b.account_id == account
                    && b.world_identity_scope == config.world_identity_scope
                    && b.player_id == entry.request.owner_binding
            })
        {
            return Err("owner migration is absent from the complete identity snapshot".into());
        }
    }
    let manifest = ParticipantMigrationManifestV1 {
        api_rev: 1,
        policy_digest: policy_digest(&policy)?,
        world_id: policy.world_id.clone(),
        genesis_digest: policy.genesis_digest.clone(),
        branch_id: prepared.branch_id.clone(),
        reorg_epoch: prepared.reorg_epoch,
        identity_namespace: namespace,
        source_snapshot_epoch: snapshot.source_snapshot_epoch,
        source_watermark: snapshot.source_watermark.clone(),
        source_snapshot_digest: snapshot.source_snapshot_digest.clone(),
        complete: true,
        previous_registry_digest: prepared.registry_digest.clone(),
        legacy_owner_state_digest: prepared.legacy_owner_state_digest.clone(),
        entries,
    };
    let issuer_hex = zeroize::Zeroizing::new(hex::encode(issuer));
    let operator_hex = zeroize::Zeroizing::new(hex::encode(operator));
    let authorization = sign_read_request(
        devlocal_policy_signing_domain(policy.generation),
        DevLocalParticipantPolicyAuthorizationV1 {
            policy: policy.clone(),
            manifest: sign_read_request(
                PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,
                manifest,
                &issuer_hex,
            )?,
        },
        &operator_hex,
    )?;
    // Validation can append events only to this detached in-memory World.
    world
        .install_devlocal_participant_policy(policy, authorization.clone())
        .map_err(|e| format!("authorization candidate rejected: {e:?}"))?;
    Ok(authorization)
}

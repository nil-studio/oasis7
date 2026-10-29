//! Explicit, fresh-only DevLocal provider and canonical owner provisioning.
use super::{Options, encode, files};
use oasis7::world_service::{WorldIdentity, sign_read_request};
use oasis7_runtime::{
    Action, LocalTestProviderAuthorityConfig, LocalTestProviderSessionMode,
    ProviderBackedBootstrapAuthorityV1, World, blake3_hex,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const PROFILE: &str = "devlocal-provider-v1";
const MAX_ALLOWANCE: u64 = 128;
const PROVIDER_OPTIONS: [&str; 6] = [
    "--provider-wasm",
    "--provider-metadata",
    "--provider-agent-id",
    "--provider-owner-binding",
    "--provider-finality-block-hash",
    "--provider-allowance",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Inputs {
    pub(super) wasm: PathBuf,
    pub(super) metadata: PathBuf,
    pub(super) agent_id: String,
    pub(super) owner_binding: String,
    pub(super) finality_block_hash: String,
    pub(super) allowance: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Commitment {
    profile: String,
    inputs: Inputs,
    wasm_hash: String,
    source_hash: String,
    build_manifest_hash: String,
    metadata_hash: String,
    owner_public_key: String,
    executor_public_key: String,
    authority_hash: String,
    delegation_hash: String,
}
pub(super) struct Prepared {
    pub(super) commitment: Commitment,
    owner: [u8; 32],
    executor: [u8; 32],
    authority: Vec<u8>,
    delegation: Vec<u8>,
}

pub(super) fn parse(values: &BTreeMap<&str, &str>) -> Result<Option<Inputs>, String> {
    let profile = values.get("--profile").copied().unwrap_or("public-read-v1");
    if profile == "public-read-v1" {
        if PROVIDER_OPTIONS.iter().any(|key| values.contains_key(key)) {
            return Err("provider inputs require explicit devlocal-provider-v1 profile".into());
        }
        return Ok(None);
    }
    if profile != PROFILE {
        return Err("unsupported preview profile".into());
    }
    let required = |key| {
        values
            .get(key)
            .copied()
            .ok_or_else(|| format!("missing {key}"))
    };
    let input = Inputs {
        wasm: required("--provider-wasm")?.into(),
        metadata: required("--provider-metadata")?.into(),
        agent_id: required("--provider-agent-id")?.into(),
        owner_binding: required("--provider-owner-binding")?.into(),
        finality_block_hash: required("--provider-finality-block-hash")?.into(),
        allowance: required("--provider-allowance")?
            .parse()
            .map_err(|_| "invalid provider allowance")?,
    };
    input.validate()?;
    Ok(Some(input))
}
impl Inputs {
    fn validate(&self) -> Result<(), String> {
        for path in [&self.wasm, &self.metadata] {
            files::normalized(path)?;
        }
        for id in [&self.agent_id, &self.owner_binding] {
            if id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
            {
                return Err("provider agent/owner must be exact bounded identifiers".into());
            }
        }
        let hash = self
            .finality_block_hash
            .strip_prefix("blake3:")
            .ok_or("explicit DevLocal finality marker requires blake3:64-hex")?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.allowance == 0
            || self.allowance > MAX_ALLOWANCE
        {
            return Err("invalid DevLocal marker or bounded allowance".into());
        }
        Ok(())
    }
}

pub(super) fn prepare(
    world: &mut World,
    options: &Options,
    input: &Inputs,
) -> Result<Prepared, String> {
    input.validate()?;
    if !world.state().agents.contains_key(&input.agent_id)
        || world.current_agent_owner(&input.agent_id).is_some()
    {
        return Err("provider preview requires an existing unowned canonical preset Agent".into());
    }
    let (artifact, metadata_hash) = super::super::local_provider_setup::load_module_artifact_bound(
        &input.wasm,
        &input.metadata,
    )?;
    if !artifact.wasm_bytes.starts_with(b"\0asm\x01\0\0\0") {
        return Err(
            "provider preview requires a stable binary WASM artifact and build metadata".into(),
        );
    }
    let mut config = LocalTestProviderAuthorityConfig::hosted_public_join(
        &input.agent_id,
        &input.owner_binding,
        &input.finality_block_hash,
    );
    config.session_mode = LocalTestProviderSessionMode::Loopback;
    config.presenter.session_id = Some(format!("loopback:{}", input.agent_id));
    config.allowance = input.allowance;
    world
        .install_local_test_provider_module(&config, &artifact)
        .map_err(|error| match error {
            oasis7_runtime::WorldError::DistributedValidationFailed { reason } => {
                format!("preview provider module admission rejected: {reason}")
            }
            _ => "preview provider module admission rejected by Runtime".into(),
        })?;
    let epoch = world.state().time
        / world
            .governance_execution_policy()
            .epoch_length_ticks
            .max(1);
    world
        .bind_cognition_runtime(
            &options.world_id,
            "main",
            epoch,
            Some(input.finality_block_hash.clone()),
            "pending",
            0,
        )
        .map_err(|_| "preview provider Runtime binding rejected")?;
    let provisioning = world
        .initialize_local_test_provider_authority(config, artifact.clone())
        .map_err(|_| "preview provider authority admission rejected")?;
    let mut owner = [0u8; 32];
    let mut executor = [0u8; 32];
    getrandom::fill(&mut owner)
        .and_then(|_| getrandom::fill(&mut executor))
        .map_err(|_| "preview provider entropy unavailable")?;
    let public = |seed: &[u8; 32]| {
        hex::encode(
            ed25519_dalek::SigningKey::from_bytes(seed)
                .verifying_key()
                .to_bytes(),
        )
    };
    let owner_public_key = public(&owner);
    let executor_public_key = public(&executor);
    world.submit_action(Action::ClaimStarterOc {
        agent_id: input.agent_id.clone(),
        player_id: input.owner_binding.clone(),
        public_key: Some(owner_public_key.clone()),
    });
    world
        .step()
        .map_err(|_| "canonical preview starter claim execution failed")?;
    let resolved = world
        .current_agent_owner(&input.agent_id)
        .ok_or("canonical preview owner claim rejected")?;
    if resolved.player_id != input.owner_binding
        || resolved.public_key.as_ref() != Some(&owner_public_key)
        || world
            .current_agent_identity(&input.agent_id)
            .is_none_or(|identity| {
                identity.owner_binding != input.owner_binding || identity.generation != 1
            })
        || world.main_token_liquid_balance(&input.agent_id) == 0
    {
        return Err("preview canonical owner/Agent/funding differs from request".into());
    }
    let delegation = sign_read_request(
        "delegation",
        oasis7::world_service::AgentSignerDelegationChangeV1 {
            world_id: options.world_id.clone(),
            branch_id: "main".into(),
            agent_id: input.agent_id.clone(),
            owner_binding: input.owner_binding.clone(),
            agent_identity_generation: 1,
            generation: 1,
            delegate_public_key: executor_public_key.clone(),
            revoked: false,
            nonce: 1,
        },
        &hex::encode(owner),
    )?;
    world.apply_agent_signer_delegation(&delegation)?;
    let authority = encode(json!(provisioning.bootstrap_authority))?;
    let delegation = encode(json!(delegation))?;
    let commitment = Commitment {
        profile: PROFILE.into(),
        inputs: input.clone(),
        wasm_hash: provisioning.wasm_hash,
        source_hash: artifact.source_hash,
        build_manifest_hash: artifact.build_manifest_hash,
        metadata_hash,
        owner_public_key,
        executor_public_key,
        authority_hash: blake3_hex(&authority),
        delegation_hash: blake3_hex(&delegation),
    };
    verify_initial(world, &commitment)?;
    Ok(Prepared {
        commitment,
        owner,
        executor,
        authority,
        delegation,
    })
}

pub(super) fn verify_initial(world: &World, commitment: &Commitment) -> Result<(), String> {
    let context = world.canonical_agent_service_context(&commitment.inputs.agent_id)?;
    if context.agent_id != commitment.inputs.agent_id {
        return Err("provider Agent context differs".into());
    }
    oasis7_runtime::service_admission::agent_authority::validate_agent_signer(
        world,
        &commitment.inputs.agent_id,
        &commitment.executor_public_key,
        1,
    )?;
    Ok(())
}

pub(super) fn matches_inputs(
    input: Option<&Inputs>,
    commitment: Option<&Commitment>,
) -> Result<bool, String> {
    match (input, commitment) {
        (None, None) => Ok(true),
        (Some(input), Some(c)) => {
            input.validate()?;
            let (artifact, metadata_hash) =
                super::super::local_provider_setup::load_module_artifact_bound(
                    &input.wasm,
                    &input.metadata,
                )?;
            Ok(c.profile == PROFILE
                && &c.inputs == input
                && c.source_hash == artifact.source_hash
                && c.build_manifest_hash == artifact.build_manifest_hash
                && c.wasm_hash == hex::encode(Sha256::digest(&artifact.wasm_bytes))
                && c.metadata_hash == metadata_hash)
        }
        _ => Ok(false),
    }
}

fn environment(
    options: &Options,
    identity: &WorldIdentity,
    service: &str,
    c: &Commitment,
    executor: &[u8; 32],
) -> String {
    format!(
        "OASIS7_WORLD_SERVICE_ENDPOINT=http://{}\nOASIS7_WORLD_SERVICE_PUBLIC_KEY={}\nOASIS7_WORLD_SERVICE_WORLD_ID={}\nOASIS7_WORLD_SERVICE_GENESIS_DIGEST={}\nOASIS7_WORLD_SERVICE_SCOPE=agent:{}\nOASIS7_WORLD_SERVICE_READ_PRIVATE_KEY={}\nOASIS7_WORLD_SERVICE_AGENT_PRIVATE_KEY={}\nOASIS7_WORLD_SERVICE_AGENT_DELEGATION_GENERATION=1\n",
        options.status_bind,
        service,
        options.world_id,
        identity.genesis_digest,
        c.inputs.agent_id,
        hex::encode(executor),
        hex::encode(executor)
    )
}

pub(super) fn publish(
    root: &Path,
    options: &Options,
    identity: &WorldIdentity,
    service: &str,
    prepared: &Prepared,
) -> Result<(), String> {
    for (name, bytes) in [
        ("owner.key", prepared.owner.as_slice()),
        ("executor.key", prepared.executor.as_slice()),
        ("provider-authority.json", prepared.authority.as_slice()),
        ("executor-delegation.json", prepared.delegation.as_slice()),
    ] {
        files::write_new(&root.join("private").join(name), bytes)?;
    }
    files::write_new(
        &root.join("private/agent-viewer.env"),
        environment(
            options,
            identity,
            service,
            &prepared.commitment,
            &prepared.executor,
        )
        .as_bytes(),
    )
}

pub(super) fn verify_outputs(
    root: &Path,
    options: &Options,
    identity: &WorldIdentity,
    service: &str,
    c: &Commitment,
    owner: u32,
) -> Result<(), String> {
    for name in [
        "owner.key",
        "executor.key",
        "provider-authority.json",
        "executor-delegation.json",
        "agent-viewer.env",
    ] {
        files::verify_private(&root.join("private").join(name), false, owner)?;
    }
    let secret = |name: &str, expected: &str| -> Result<[u8; 32], String> {
        let seed: [u8; 32] = files::read_bounded(&root.join("private").join(name), 32)?
            .try_into()
            .map_err(|_| "invalid provider private seed")?;
        if hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&seed)
                .verifying_key()
                .to_bytes(),
        ) != expected
        {
            return Err("provider private/public identity mismatch".into());
        }
        Ok(seed)
    };
    let _ = secret("owner.key", &c.owner_public_key)?;
    let executor = secret("executor.key", &c.executor_public_key)?;
    let authority =
        files::read_bounded(&root.join("private/provider-authority.json"), 1024 * 1024)?;
    let _: ProviderBackedBootstrapAuthorityV1 =
        serde_json::from_slice(&authority).map_err(|_| "provider authority output invalid")?;
    if blake3_hex(&authority) != c.authority_hash
        || blake3_hex(&files::read_bounded(
            &root.join("private/executor-delegation.json"),
            64 * 1024,
        )?) != c.delegation_hash
        || files::read_bounded(&root.join("private/agent-viewer.env"), 8192)?
            != environment(options, identity, service, c, &executor).as_bytes()
    {
        return Err("provider retained authority/delegation/environment differs".into());
    }
    Ok(())
}

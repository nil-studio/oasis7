//! Existing-world activation material. This code never creates a World/genesis.
use super::{
    controlled_capture as capture,
    formal_package::{execution_policy_material, insert, validate_modules},
    to_cbor,
};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::controlled_authority::formal::{
    business_index::BusinessIndexNodeReader,
    business_material::{FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1, FormalBusinessResultEnvelopeV1},
};
use oasis7_distfs::controlled_authority::{
    formal::{endpoint::FormalCandidateMaterialV1, *},
    replicated_protocol::{ArtifactObject, ArtifactRole, ClosedRecord},
};
use oasis7_runtime::{ReleaseSecurityPolicy, World, blake3_hex};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
struct EmptyBusinessIndex;
impl BusinessIndexNodeReader for EmptyBusinessIndex {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InitialNodeBootstrapV1 {
    pub height: u64,
    pub consensus_block_hash: String,
    pub execution_block_hash: String,
    pub execution_state_root: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InitialResultV1 {
    pub schema_version: u32,
    pub domain: String,
    pub world: FormalWorldIdentityV1,
    pub state_root: String,
    pub journal_ref: String,
    pub logical_tick: u64,
    pub node_bootstrap: InitialNodeBootstrapV1,
}

pub(crate) struct InitialWorldClosure {
    world: FormalWorldIdentityV1,
    state_root: String,
    governing_manifest_digest: String,
    execution_policy_digest: String,
    executable_digest: String,
    roots: BTreeMap<ArtifactRole, String>,
    objects: BTreeMap<String, ArtifactObject>,
    event_root: String,
    projection_root: String,
    result_root: String,
    business_contract: bool,
    retained_history_initialized: bool,
}
impl InitialWorldClosure {
    /// Extend the unsigned approval input with exact external native bytes.
    /// The original manifest remains reachable; historical v1 approvals keep
    /// their original identities and cannot acquire this requirement silently.
    pub(crate) fn with_native_environment(
        mut self,
        native: &oasis7_formal_protocol::native_environment::NativeExecutionEnvironmentV1,
    ) -> Result<Self, String> {
        use oasis7_formal_protocol::native_environment::NativeEnvironmentRole;
        native.validate()?;
        if !native.components.iter().any(|component| {
            component.role == NativeEnvironmentRole::Executable
                && component.content_digest == self.executable_digest
        }) {
            return Err("initial native executable differs from approved execution policy".into());
        }
        if self
            .roots
            .contains_key(&ArtifactRole::NativeExecutionEnvironment)
        {
            return Err("initial native environment already attached".into());
        }
        let native_ref = insert(&mut self.objects, native.encode()?, vec![]);
        let old_manifest = self
            .roots
            .get(&ArtifactRole::ExecutionManifest)
            .ok_or("initial execution manifest missing")?
            .clone();
        let manifest = insert(
            &mut self.objects,
            to_cbor((
                "oasis7.formal.initial_execution_manifest.v2",
                &self.world,
                &old_manifest,
                &native_ref,
            ))?,
            vec![old_manifest, native_ref.clone()],
        );
        self.roots
            .insert(ArtifactRole::NativeExecutionEnvironment, native_ref);
        self.roots.insert(ArtifactRole::ExecutionManifest, manifest);
        Ok(self)
    }
    /// Prepare commitments for an external activation authority to authorize.
    /// The supplied checkpoint must already carry its exact chain/world/genesis.
    pub(crate) fn from_existing_world(
        world: &World,
        identity: &FormalWorldIdentityV1,
        release: &ReleaseSecurityPolicy,
    ) -> Result<Self, String> {
        Self::from_existing_world_inner(world, identity, release, None, None)
    }
    pub(crate) fn from_existing_world_with_node_bootstrap(
        world: &World,
        identity: &FormalWorldIdentityV1,
        release: &ReleaseSecurityPolicy,
        bootstrap: InitialNodeBootstrapV1,
    ) -> Result<Self, String> {
        if bootstrap.height == 0
            || bootstrap.height != world.state().time
            || bootstrap.execution_state_root != blake3_hex(&to_cbor(world.snapshot())?)
            || bootstrap.consensus_block_hash.trim().is_empty()
            || bootstrap.execution_block_hash.trim().is_empty()
        {
            return Err(
                "formal initial node bootstrap differs from existing World boundary".into(),
            );
        }
        Self::from_existing_world_inner(world, identity, release, Some(bootstrap), None)
    }
    /// Stage a fresh genesis boundary for external approval. The resulting
    /// initial record commits these exact hashes and the zero-height Snapshot;
    /// Node startup must consume its qualified initial proof via the genesis API.
    pub(crate) fn from_genesis_world_with_node_bootstrap(
        world: &World,
        identity: &FormalWorldIdentityV1,
        release: &ReleaseSecurityPolicy,
        bootstrap: InitialNodeBootstrapV1,
    ) -> Result<Self, String> {
        if bootstrap.height != 0
            || world.state().time != 0
            || bootstrap.execution_state_root != blake3_hex(&to_cbor(world.snapshot())?)
            || bootstrap.consensus_block_hash.trim().is_empty()
            || bootstrap.execution_block_hash.trim().is_empty()
        {
            return Err("formal genesis bootstrap differs from fresh World boundary".into());
        }
        Self::from_existing_world_inner(world, identity, release, Some(bootstrap), None)
    }
    #[cfg(test)]
    pub(crate) fn from_existing_world_for_test_executable(
        world: &World,
        identity: &FormalWorldIdentityV1,
        release: &ReleaseSecurityPolicy,
        bootstrap: InitialNodeBootstrapV1,
        executable: &std::path::Path,
    ) -> Result<Self, String> {
        if bootstrap.height == 0
            || bootstrap.height != world.state().time
            || bootstrap.execution_state_root != blake3_hex(&to_cbor(world.snapshot())?)
            || bootstrap.consensus_block_hash.trim().is_empty()
            || bootstrap.execution_block_hash.trim().is_empty()
        {
            return Err(
                "formal shipping test bootstrap differs from existing World boundary".into(),
            );
        }
        Self::from_existing_world_inner(
            world,
            identity,
            release,
            Some(bootstrap),
            Some(test_execution_policy_material(executable, release)?),
        )
    }
    fn from_existing_world_inner(
        world: &World,
        identity: &FormalWorldIdentityV1,
        release: &ReleaseSecurityPolicy,
        bootstrap: Option<InitialNodeBootstrapV1>,
        test_policy: Option<(Vec<u8>, String, String)>,
    ) -> Result<Self, String> {
        identity.validate().map_err(|e| e.to_string())?;
        let snapshot = world.snapshot();
        if !snapshot
            .capability_revocation_state
            .world_service_results
            .is_empty()
        {
            return Err(
                "formal initial activation requires an explicit legacy result migration".into(),
            );
        }
        let binding = &snapshot.chain_resource_manifest;
        if binding.world_id != identity.world_id
            || binding.chain_id != identity.chain_id
            || binding.genesis_ref.as_deref() != Some(identity.genesis_digest.as_str())
        {
            return Err(
                "formal initialization requires an existing identity-bound checkpoint".into(),
            );
        }
        validate_modules(&snapshot)?;
        let mut restored =
            oasis7::runtime_host::world::from_snapshot(snapshot.clone(), world.journal().clone())
                .map_err(|e| format!("{e:?}"))?
                .with_release_security_policy(release.clone());
        if snapshot.state.retained_history_root.is_some() {
            // Explicit initial preparation may inspect only the canonical empty
            // index. Restore its process-local reader without minting lineage.
            restored.initialize_empty_formal_history()?;
        }
        let snapshot_bytes = to_cbor(&snapshot)?;
        if to_cbor(restored.snapshot())? != snapshot_bytes {
            return Err("formal initial typed checkpoint changed on restoration".into());
        }
        let world = &restored;
        let governing = world
            .current_cognition_runtime_binding()
            .map_err(|e| format!("{e:?}"))?
            .runtime_manifest_hash
            .to_string()
            .strip_prefix("blake3:")
            .ok_or("formal initial governing hash encoding")?
            .to_owned();
        let (policy, executable, policy_digest) = match test_policy {
            Some(material) => material,
            None => execution_policy_material(release)?,
        };
        let mut objects = BTreeMap::new();
        let state_root = insert(&mut objects, snapshot_bytes, vec![]);
        let journal = insert(&mut objects, to_cbor(world.journal())?, vec![]);
        let business_contract = bootstrap.is_some();
        let mut result_bytes = match bootstrap {
            Some(node_bootstrap) => to_cbor(InitialResultV1 {
                schema_version: 1,
                domain: "oasis7.formal.initial_node_result.v1".into(),
                world: identity.clone(),
                state_root: state_root.clone(),
                journal_ref: journal.clone(),
                logical_tick: snapshot.state.time,
                node_bootstrap,
            })?,
            None => to_cbor((
                "oasis7.formal.initial_result.v1",
                identity,
                &state_root,
                &journal,
                snapshot.state.time,
            ))?,
        };
        let mut business_root = None;
        let result_references = vec![state_root.clone(), journal.clone()];
        if business_contract {
            let update = oasis7::world_service::formal_result::FormalBusinessResultTreeV1::for_initial_world(world, identity, &EmptyBusinessIndex)?;
            business_root = Some(update.tree.root()?);
            // Node bytes are committed inside the one bounded delta envelope.
            // Endpoints derive and durably store their raw logical CAS objects;
            // no cumulative history or per-node artifact list is embedded.
            result_bytes = FormalBusinessResultEnvelopeV1 {
                schema_version: 1,
                domain: FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1.into(),
                execution_result_cbor: result_bytes,
                tree_descriptor_cbor: update.tree.encode_cbor()?,
                delta: update.delta,
                retained_history_delta: None,
            }
            .encode()
            .map_err(|e| e.to_string())?;
        }
        let result = insert(&mut objects, result_bytes, result_references);
        let result_root = business_root.unwrap_or_else(|| result.clone());
        let nonce = insert(
            &mut objects,
            to_cbor((
                "oasis7.formal.nonce_state.v1",
                &snapshot.capability_nonce_records,
                &snapshot.capability_revocation_state.world_service_results,
                &snapshot.state.authenticated_collect_data_last_nonces,
                &snapshot.state.agent_intent_ledger,
            ))?,
            vec![],
        );
        let outbox = insert(
            &mut objects,
            to_cbor((&snapshot.pending_effects, &snapshot.inflight_effects))?,
            vec![],
        );
        let rules = insert(
            &mut objects,
            to_cbor((&snapshot.manifest, &snapshot.policies, release))?,
            vec![],
        );
        let mut wasm_refs = BTreeMap::new();
        for (hash, bytes) in &snapshot.module_artifact_bytes {
            wasm_refs.insert(hash.clone(), insert(&mut objects, bytes.clone(), vec![]));
        }
        let wasm = insert(
            &mut objects,
            to_cbor((&snapshot.module_registry, &wasm_refs))?,
            wasm_refs.values().cloned().collect(),
        );
        let mut roots = BTreeMap::from([
            (ArtifactRole::Snapshot, state_root.clone()),
            (ArtifactRole::Journal, journal),
            (ArtifactRole::Result, result),
            (ArtifactRole::NonceIndex, nonce),
            (ArtifactRole::EffectOutbox, outbox),
            (ArtifactRole::Rules, rules),
            (ArtifactRole::Wasm, wasm),
        ]);
        let manifest = insert(
            &mut objects,
            to_cbor((
                "oasis7.formal.initial_execution_manifest.v1",
                identity,
                &governing,
                &policy_digest,
                &policy,
                &executable,
                release,
                &roots,
            ))?,
            roots.values().cloned().collect(),
        );
        roots.insert(ArtifactRole::ExecutionManifest, manifest);
        Ok(Self {
            world: identity.clone(),
            state_root,
            governing_manifest_digest: governing,
            execution_policy_digest: policy_digest,
            executable_digest: executable,
            roots,
            objects,
            event_root: blake3_hex(&to_cbor(&world.journal().events)?),
            projection_root: oasis7::world_service::formal_projection::commitments_for_world(
                world,
            )?
            .root()?,
            result_root,
            business_contract,
            retained_history_initialized: snapshot.state.retained_history_root.is_some(),
        })
    }
    pub(crate) fn activation_body(
        &self,
        authority: AuthorityDefinitionV1,
        recovery_policy: RecoveryPolicyV1,
    ) -> Result<InitialActivationBodyV1, String> {
        authority
            .verify_policy(&recovery_policy)
            .map_err(|e| e.to_string())?;
        if authority.profile_version == 2
            && (!self.business_contract || !self.retained_history_initialized)
        {
            return Err(
                "formal profile 2 activation requires explicit retained history, business index and approved Node bootstrap before approval"
                    .into(),
            );
        }
        if authority.world != self.world
            || authority.epoch != 1
            || authority.valid_from_position != 1
            || authority.predecessor_authority_id.is_some()
        {
            return Err("formal initial authority/world/start mismatch".into());
        }
        Ok(InitialActivationBodyV1 {
            schema_version: if self
                .roots
                .contains_key(&ArtifactRole::NativeExecutionEnvironment)
            {
                2
            } else {
                1
            },
            authority,
            recovery_policy,
            initial_state_root: self.state_root.clone(),
            runtime_manifest_digest: self.roots[&ArtifactRole::ExecutionManifest].clone(),
            governing_manifest_digest: self.governing_manifest_digest.clone(),
            execution_policy_digest: self.execution_policy_digest.clone(),
            artifact_roots: self.roots.clone(),
        })
    }
    /// Only independently trusted, already signed activation can be admitted.
    pub(crate) fn authorize(
        mut self,
        certificate: &SignedInitialActivationV1,
        trust: &InitialActivationTrustV1,
        writer: &SigningKey,
    ) -> Result<(VerifiedAuthorityV1, FormalCandidateMaterialV1), String> {
        let authority = verify_initial_activation(certificate, trust).map_err(|e| e.to_string())?;
        if hex::encode(writer.verifying_key().as_bytes())
            != authority.definition().writer_public_key
            || certificate.body
                != self
                    .activation_body(authority.definition().clone(), authority.policy().clone())?
        {
            return Err("formal initial original key or exact approved closure differs".into());
        }
        let payload = insert(
            &mut self.objects,
            initial_activation_input_bytes(certificate).map_err(|e| e.to_string())?,
            vec![],
        );
        self.roots.insert(ArtifactRole::Input, payload.clone());
        let material = ClosedRecord {
            payload_digest: payload.clone(),
            before_state_root: self.state_root.clone(),
            after_state_root: self.state_root.clone(),
            roots: self.roots.clone(),
            objects: self.objects.into_values().collect(),
        };
        material.validate().map_err(|e| e.to_string())?;
        capture::validate_package_budget(&to_cbor(&material)?)?;
        let canonical = CanonicalRecordV1 {
            schema_version: if self
                .roots
                .contains_key(&ArtifactRole::NativeExecutionEnvironment)
            {
                5
            } else {
                1
            },
            durable_set_transition_digest: None,
            kind: FormalRecordKindV1::InitialActivation,
            world: self.world.clone(),
            position: 1,
            parent_record_id: self
                .world
                .genesis_head()
                .map_err(|e| e.to_string())?
                .record_id,
            original_authority_id: authority.authority_id().map_err(|e| e.to_string())?,
            original_epoch: 1,
            activation_ref: authority.activation_ref().into(),
            request_digest: blake3_hex(&to_cbor((
                "oasis7.formal.initial_request.v1",
                &self.world,
                authority.activation_ref(),
            ))?),
            payload_digest: payload,
            frozen_reservation_id: None,
            runtime_manifest_digest: certificate.body.runtime_manifest_digest.clone(),
            governing_manifest_digest: self.governing_manifest_digest,
            execution_policy_digest: self.execution_policy_digest,
            before_state_root: self.state_root.clone(),
            after_state_root: self.state_root,
            action_root: oasis7_node::compute_consensus_action_root(&[])
                .map_err(|e| format!("{e:?}"))?,
            event_root: self.event_root,
            result_root: self.result_root,
            projection_root: self.projection_root,
            artifact_roots: self.roots,
            handoff: None,
        };
        verify_record_material(&canonical, &material).map_err(|e| e.to_string())?;
        let candidate = SignedCanonicalCandidateV1 {
            writer_signature_hex: hex::encode(
                writer
                    .sign(
                        &canonical_candidate_signing_bytes(&canonical)
                            .map_err(|e| e.to_string())?,
                    )
                    .to_bytes(),
            ),
            record: canonical,
        };
        verify_formal_candidate(
            &candidate,
            None,
            &authority,
            None,
            None,
            FormalCompletionBranchV1::Normal,
        )
        .map_err(|e| e.to_string())?;
        Ok((
            authority,
            FormalCandidateMaterialV1 {
                candidate,
                material,
                frozen: None,
                frozen_material: None,
                branch: FormalCompletionBranchV1::Normal,
            },
        ))
    }
}

/// Test setup pins the real shipping image before any external C0 is signed.
#[cfg(test)]
pub(crate) fn test_execution_policy_material(
    executable: &std::path::Path,
    release: &ReleaseSecurityPolicy,
) -> Result<(Vec<u8>, String, String), String> {
    if !executable.is_absolute() {
        return Err("shipping fixture executable must be absolute".into());
    }
    let path = std::fs::canonicalize(executable).map_err(|e| e.to_string())?;
    let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("shipping fixture executable must be regular".into());
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = std::io::Read::read(&mut file, &mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let executable = hasher.finalize().to_hex().to_string();
    let policy = to_cbor(crate::controlled_execution_policy::GuardedExecutionPolicy::current())?;
    let digest = blake3_hex(&to_cbor((
        "oasis7.formal.execution_policy.v1",
        &policy,
        &executable,
        release,
    ))?);
    Ok((policy, executable, digest))
}

//! Explicit Runtime-owned provider capability fixture for in-crate live tests.
//!
//! This is intentionally test-only.  Production worlds stay fail-closed until
//! an external authority installs the same proof-bearing records through the
//! public admission APIs.

use super::super::capability_authorization::{
    CapabilityAuthorityFinalityBinding, CapabilityAuthorityFinalityProof, CapabilityAuthorityRecord,
};
use super::super::governance::GovernanceFinalityEpochSnapshot;
use super::super::{
    CapabilityInvocationContext, CognitionProvisioningRequestV1, GovernanceFinalityCertificate,
    Manifest, ModuleAbiContract, ModuleActivation, ModuleArtifactIdentity, ModuleChangeSet,
    ModuleKind, ModuleLimits, ModuleManifest, ModuleRole, ProposalDecision, WorldError,
};
use super::World;
use super::provider_backed_bootstrap::ProviderBackedBootstrapAuthorityV1;
use crate::capability_authorization::CapabilityAgentIdentity;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_wasm_abi::{
    CapabilityAudience, CapabilityGrantV2, CapabilityIssuer, CapabilityPresenter, CapabilityScope,
    CapabilitySubject, ModuleCommandDeclaration, ModuleSchemaDeclarations,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

const ISSUER_ID: &str = "governance.local.finality.signer.1";
const FINALITY_SIGNER_ID: &str = "governance.local.finality.signer.2";
const ISSUER_SEED: &[u8] = b"oasis7-governance-local-finality-signer-1-v1";
const FINALITY_SIGNER_SEED: &[u8] = b"oasis7-governance-local-finality-signer-2-v1";
const MODULE_ID: &str = "module.runtime.provider-fixture";
const MODULE_VERSION: &str = "1.0.0";
const MODULE_SCHEMA_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const MODULE_WASM_HASH: &str = "d794dc476ae52ee4ca78f9ee1662b6f384a5185124db90495f6a64ee579ff4a3";
const MODULE_WASM_BYTES: &[u8] = b"runtime-provider-capability-fixture-v1";

impl World {
    /// Install a complete, proof-bearing provider capability fixture for an
    /// already-bound in-crate live test World.  Viewer only opts into this
    /// seam; it does not construct authority, grant, or invocation fields.
    #[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
    pub fn install_test_provider_capability_fixture(
        &mut self,
        agent_id: &str,
    ) -> Result<CapabilityInvocationContext, WorldError> {
        let mut staged = self.clone();
        let invocation = staged.install_test_provider_capability_fixture_inner(agent_id, true)?;
        *self = staged;
        Ok(invocation)
    }

    /// Install the same proof-bearing provider capability fixture without a
    /// legacy cognition balance. This lets Runtime bootstrap tests exercise
    /// the explicit provisioning path while keeping the production fixture
    /// behavior unchanged.
    #[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
    pub fn install_test_provider_capability_fixture_without_cognition_balance(
        &mut self,
        agent_id: &str,
    ) -> Result<CapabilityInvocationContext, WorldError> {
        let mut staged = self.clone();
        let invocation = staged.install_test_provider_capability_fixture_inner(agent_id, false)?;
        *self = staged;
        Ok(invocation)
    }

    /// Build a complete explicit ProviderBacked bootstrap bundle from the
    /// proof-bearing in-crate fixture. The helper is test-only; production
    /// bundles must come from the externally governed authority path.
    #[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
    pub fn test_provider_backed_bootstrap_authority(
        &self,
        agent_id: &str,
        provision_id: &str,
        authority_context: &str,
        allowance: u64,
    ) -> Result<ProviderBackedBootstrapAuthorityV1, WorldError> {
        let identity = self
            .capability_revocation_state
            .agent_identities
            .get(agent_id)
            .cloned()
            .ok_or_else(|| fixture_error("fixture identity is missing"))?;
        let invocation_context = self
            .capability_invocation_contexts
            .values()
            .find(|context| {
                context.grant_id
                    == self
                        .capability_grants_v2
                        .values()
                        .find_map(|encoded| {
                            serde_json::from_value::<CapabilityGrantV2>(encoded.clone())
                                .ok()
                                .filter(|grant| {
                                    matches!(
                                        &grant.subject,
                                        CapabilitySubject::Agent { agent_id: subject_id, .. }
                                            if subject_id == agent_id
                                    )
                                })
                                .map(|grant| grant.grant_id)
                        })
                        .unwrap_or_default()
            })
            .cloned()
            .ok_or_else(|| fixture_error("fixture invocation context is missing"))?;
        let grant: CapabilityGrantV2 = self
            .capability_grants_v2
            .get(&invocation_context.grant_id)
            .cloned()
            .ok_or_else(|| fixture_error("fixture grant is missing"))
            .and_then(|encoded| {
                serde_json::from_value(encoded)
                    .map_err(|error| fixture_error(format!("fixture grant decode: {error}")))
            })?;
        let authority_record = self
            .capability_revocation_state
            .authority_records
            .get(&grant.issuer.issuer_id)
            .cloned()
            .ok_or_else(|| fixture_error("fixture authority record is missing"))?;
        let authority_finality_proof = self
            .capability_revocation_state
            .authority_finality_proofs
            .get(&grant.issuer.issuer_id)
            .cloned()
            .ok_or_else(|| fixture_error("fixture authority proof is missing"))?;
        let binding = self
            .current_cognition_runtime_binding()
            .map_err(|error| fixture_error(format!("fixture Runtime binding: {error:?}")))?;
        let request = CognitionProvisioningRequestV1::new(
            provision_id,
            identity.owner_binding.clone(),
            identity.owner_binding.clone(),
            identity.generation,
            binding.world_id.clone(),
            binding.branch_id.clone(),
            binding.reorg_epoch,
            allowance,
            authority_context,
        );
        Ok(ProviderBackedBootstrapAuthorityV1 {
            agent_id: agent_id.to_string(),
            owner_binding: identity.owner_binding.clone(),
            owner_generation: identity.generation,
            world_id: binding.world_id,
            branch_id: binding.branch_id,
            reorg_epoch: binding.reorg_epoch,
            identity,
            authority_record,
            authority_finality_proof,
            grant,
            invocation_context,
            provision_id: request.provision_id,
            authority_context: request.authority_context,
            authority_digest: request.authority_digest,
            provisioning_digest: request.provisioning_digest,
            allowance: request.allowance,
        })
    }

    /// Install one additional valid command grant for a provider fixture.
    ///
    /// The grant deliberately shares the agent, authority, audience, and
    /// command scope with the primary fixture grant while carrying a distinct
    /// nonce. This lets bootstrap tests exercise the ambiguity boundary where
    /// catalog selection must not silently replace the caller-supplied grant.
    #[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
    pub fn install_test_provider_additional_capability_grant(
        &mut self,
        agent_id: &str,
        nonce_suffix: &str,
    ) -> Result<CapabilityGrantV2, WorldError> {
        let mut staged = self.clone();
        let identity = staged
            .capability_revocation_state
            .agent_identities
            .get(agent_id)
            .cloned()
            .ok_or_else(|| fixture_error("fixture identity is missing"))?;
        let (world_id, branch_id, finality_epoch, _) = staged.bound_runtime_identity()?;
        if nonce_suffix.trim().is_empty() {
            return Err(fixture_error("fixture additional grant nonce is required"));
        }
        let grant = staged.fixture_command_grant_with_nonce(
            agent_id,
            &identity,
            world_id.as_str(),
            branch_id.as_str(),
            finality_epoch,
            format!("runtime-test-provider:{agent_id}:additional:{nonce_suffix}"),
        )?;
        staged.register_capability_grant_v2(grant.clone())?;
        *self = staged;
        Ok(grant)
    }

    fn install_test_provider_capability_fixture_inner(
        &mut self,
        agent_id: &str,
        seed_cognition_balance: bool,
    ) -> Result<CapabilityInvocationContext, WorldError> {
        self.verify_capability_authorization_root()?;
        let (world_id, branch_id, finality_epoch, finality_block_hash) =
            self.bound_runtime_identity()?;
        if !self.state.agents.contains_key(agent_id) {
            return Err(fixture_error("provider fixture requires a live agent"));
        }
        // Provider caller tests exercise the real Runtime lease admission
        // path. Seed their account explicitly in the test fixture; production
        // worlds must provision cognition units through their own authority
        // and never receive an implicit dispatch-time refill.
        if seed_cognition_balance {
            self.set_cognition_resource_balance(agent_id, "cognition_units", 128)?;
        }
        if let Some(existing) = self
            .capability_invocation_contexts
            .values()
            .find(|context| {
                matches!(
                    &context.subject,
                    CapabilitySubject::Agent { agent_id: subject_id, .. }
                        if subject_id == agent_id
                ) && context.presenter.presenter_kind == "provider"
                    && context.audience.world_id == world_id
                    && context.audience.branch_id == branch_id
                    && context.audience.finality_epoch == finality_epoch
            })
        {
            return Ok(existing.clone());
        }

        let identity = self.install_fixture_agent_identity(agent_id)?;
        self.install_fixture_authority(
            world_id.as_str(),
            branch_id.as_str(),
            finality_epoch,
            finality_block_hash.as_deref(),
        )?;
        self.install_fixture_module()?;

        let grant = self.fixture_command_grant(
            agent_id,
            &identity,
            world_id.as_str(),
            branch_id.as_str(),
            finality_epoch,
        )?;
        self.register_capability_grant_v2(grant)?;
        let presenter = CapabilityPresenter {
            presenter_id: format!("runtime-test-provider:{agent_id}"),
            presenter_kind: "provider".to_string(),
            session_id: Some(format!("runtime-test-session:{agent_id}")),
            attestation_ref: None,
        };
        let invocation = self.install_capability_invocation_context_for_agent(
            agent_id,
            presenter,
            format!("runtime-test-response:{agent_id}"),
        )?;
        // Cognition leases charge the Runtime-authorized capability owner,
        // not the Agent subject. Keep the legacy Agent balance for unrelated
        // fixture consumers, but provision the explicit payer account through
        // the same authority API used by production bootstrap. The companion
        // `without_cognition_balance` helper intentionally skips both legacy
        // seeding and this explicit allowance so bootstrap tests can exercise
        // the provisioning handoff themselves.
        if seed_cognition_balance {
            self.provision_cognition_for_agent(
                agent_id,
                format!("runtime-test-provision:{agent_id}:{branch_id}:{finality_epoch}"),
                "runtime-test-authority",
                128,
            )?;
        }
        Ok(invocation)
    }

    fn bound_runtime_identity(&self) -> Result<(String, String, u64, Option<String>), WorldError> {
        let binding = self
            .cognition
            .get("runtime_binding")
            .and_then(|value| value.as_object())
            .ok_or_else(|| fixture_error("provider fixture requires a bound Runtime cognition"))?;
        let string_field = |name: &str| {
            binding
                .get(name)
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| fixture_error(format!("Runtime binding field {name} is required")))
        };
        let world_id = string_field("world_id")?;
        let branch_id = string_field("branch_id")?;
        let finality_epoch = binding
            .get("finality_epoch")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| fixture_error("Runtime binding finality_epoch is required"))?;
        let finality_block_hash = binding
            .get("finality_block_hash")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        Ok((world_id, branch_id, finality_epoch, finality_block_hash))
    }

    fn install_fixture_agent_identity(
        &mut self,
        agent_id: &str,
    ) -> Result<CapabilityAgentIdentity, WorldError> {
        if let Some(identity) = self
            .capability_revocation_state
            .agent_identities
            .get(agent_id)
        {
            return Ok(identity.clone());
        }
        self.install_capability_agent_identity(
            agent_id,
            format!("runtime-test-owner:{agent_id}"),
            1,
        )?;
        self.capability_revocation_state
            .agent_identities
            .get(agent_id)
            .cloned()
            .ok_or_else(|| fixture_error("fixture identity was not persisted"))
    }

    fn install_fixture_authority(
        &mut self,
        world_id: &str,
        branch_id: &str,
        finality_epoch: u64,
        finality_block_hash: Option<&str>,
    ) -> Result<(), WorldError> {
        if let Some(existing) = self
            .capability_revocation_state
            .authority_records
            .get(ISSUER_ID)
            && existing.world_id == world_id
            && existing.branch_id == branch_id
            && existing.finality_epoch == finality_epoch
            && existing.finality_block_hash
                == finality_block_hash
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("runtime-test-finality-block:{finality_epoch}"))
            && self
                .capability_revocation_state
                .authority_finality_proofs
                .contains_key(ISSUER_ID)
        {
            return Ok(());
        }
        let issuer_key = fixture_signing_key(ISSUER_SEED);
        let finality_key = fixture_signing_key(FINALITY_SIGNER_SEED);
        self.bind_node_identity(
            ISSUER_ID,
            &hex::encode(issuer_key.verifying_key().to_bytes()),
        )?;
        self.bind_node_identity(
            FINALITY_SIGNER_ID,
            &hex::encode(finality_key.verifying_key().to_bytes()),
        )?;
        self.set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id: finality_epoch,
            threshold_bps: 10_000,
            min_unique_signers: 2,
            threshold: 2,
            signer_node_ids: vec![ISSUER_ID.to_string(), FINALITY_SIGNER_ID.to_string()],
            validator_stakes: BTreeMap::from([
                (ISSUER_ID.to_string(), 100),
                (FINALITY_SIGNER_ID.to_string(), 100),
            ]),
            ..GovernanceFinalityEpochSnapshot::default()
        })?;

        let proposal = self.propose_manifest_update(
            Manifest {
                version: self.manifest.version.saturating_add(1),
                content: json!({"runtime_provider_capability_fixture": true}),
            },
            "runtime-test-fixture",
        )?;
        self.shadow_proposal(proposal)?;
        self.approve_proposal(proposal, "runtime-test-fixture", ProposalDecision::Approve)?;
        let proposal_manifest_hash = match &self
            .proposals
            .get(&proposal)
            .ok_or_else(|| fixture_error("fixture authority proposal disappeared"))?
            .status
        {
            super::super::ProposalStatus::Approved { manifest_hash, .. } => manifest_hash.clone(),
            status => {
                return Err(fixture_error(format!(
                    "fixture authority proposal is not approved: {status:?}"
                )));
            }
        };
        let snapshot = self
            .governance_finality_epoch_snapshots
            .get(&finality_epoch)
            .cloned()
            .ok_or_else(|| fixture_error("fixture finality snapshot was not persisted"))?;
        let consensus_height = self.journal.events.len() as u64 + 1;
        let min_unique_signers = snapshot.effective_min_unique_signers();
        let mut certificate_signatures = BTreeMap::new();
        for (node_id, key) in [
            (ISSUER_ID, issuer_key.clone()),
            (FINALITY_SIGNER_ID, finality_key.clone()),
        ] {
            let payload = GovernanceFinalityCertificate::signing_payload_v1(
                proposal,
                proposal_manifest_hash.as_str(),
                consensus_height,
                snapshot.epoch_id,
                snapshot.validator_set_hash.as_str(),
                snapshot.stake_root.as_str(),
                snapshot.threshold_bps,
                min_unique_signers,
                node_id,
            );
            let signature = key.sign(payload.as_slice());
            certificate_signatures.insert(
                node_id.to_string(),
                format!(
                    "{}{}",
                    GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
                    hex::encode(signature.to_bytes())
                ),
            );
        }
        let certificate = GovernanceFinalityCertificate {
            proposal_id: proposal,
            manifest_hash: proposal_manifest_hash,
            consensus_height,
            epoch_id: snapshot.epoch_id,
            validator_set_hash: snapshot.validator_set_hash,
            stake_root: snapshot.stake_root,
            threshold_bps: snapshot.threshold_bps,
            min_unique_signers,
            threshold: min_unique_signers,
            signatures: certificate_signatures,
        };
        let authority = CapabilityAuthorityRecord {
            issuer_id: ISSUER_ID.to_string(),
            issuer_kind: "governance".to_string(),
            key_id: "governance-local-finality-key-1".to_string(),
            public_key_hex: hex::encode(issuer_key.verifying_key().to_bytes()),
            issuer_key_epoch: 1,
            governance_epoch: finality_epoch,
            finalized_receipt_id: format!(
                "runtime-test-authority:{world_id}:{branch_id}:{finality_epoch}"
            ),
            authority_rotation_receipt_id: None,
            world_id: world_id.to_string(),
            branch_id: branch_id.to_string(),
            finality_epoch,
            finality_block_hash: finality_block_hash
                .map(str::to_string)
                .unwrap_or_else(|| format!("runtime-test-finality-block:{finality_epoch}")),
            finality_status: "finalized".to_string(),
            revocation_epoch: 0,
            revoked_grant_ids: BTreeSet::new(),
            superseded_by: BTreeMap::new(),
        };
        let binding =
            CapabilityAuthorityFinalityBinding::from_record(&authority).map_err(fixture_error)?;
        let mut proof = CapabilityAuthorityFinalityProof {
            proof_version: CapabilityAuthorityFinalityProof::PROOF_VERSION_V1,
            certificate,
            binding,
            signatures: BTreeMap::new(),
        };
        for (node_id, key) in [(ISSUER_ID, issuer_key), (FINALITY_SIGNER_ID, finality_key)] {
            let payload = proof.signing_payload_v1(node_id).map_err(fixture_error)?;
            let signature = key.sign(payload.as_slice());
            proof.signatures.insert(
                node_id.to_string(),
                format!(
                    "{}{}",
                    CapabilityAuthorityFinalityProof::SIGNATURE_PREFIX_ED25519_V1,
                    hex::encode(signature.to_bytes())
                ),
            );
        }
        self.install_capability_authority_record_with_finality_proof(authority, proof)
    }

    fn install_fixture_module(&mut self) -> Result<(), WorldError> {
        let manifest = ModuleManifest {
            module_id: MODULE_ID.to_string(),
            name: "Runtime provider capability fixture".to_string(),
            version: MODULE_VERSION.to_string(),
            kind: ModuleKind::Pure,
            role: ModuleRole::AgentInternal,
            wasm_hash: MODULE_WASM_HASH.to_string(),
            interface_version: "wasm-1".to_string(),
            exports: vec!["call".to_string()],
            subscriptions: Vec::new(),
            required_caps: Vec::new(),
            abi_contract: ModuleAbiContract {
                declarations: ModuleSchemaDeclarations {
                    commands: vec![ModuleCommandDeclaration {
                        namespace: "provider".to_string(),
                        name: "observe".to_string(),
                        schema_version: 1,
                        schema_hash: MODULE_SCHEMA_HASH.to_string(),
                        max_payload_bytes: 1024,
                    }],
                },
                ..ModuleAbiContract::default()
            },
            artifact_identity: Some(fixture_module_artifact_identity()),
            limits: ModuleLimits {
                max_mem_bytes: 64 * 1024,
                max_gas: 100_000,
                max_call_rate: 100,
                max_output_bytes: 4 * 1024,
                max_effects: 4,
                max_emits: 4,
            },
        };
        let changes = ModuleChangeSet {
            register: vec![manifest.clone()],
            activate: vec![ModuleActivation {
                module_id: manifest.module_id,
                version: manifest.version,
            }],
            ..ModuleChangeSet::default()
        };
        self.apply_module_changes_for_test(0, &changes, "runtime-test-fixture")?;
        self.module_artifact_bytes
            .insert(MODULE_WASM_HASH.to_string(), MODULE_WASM_BYTES.into());
        Ok(())
    }

    fn fixture_command_grant(
        &self,
        agent_id: &str,
        identity: &CapabilityAgentIdentity,
        world_id: &str,
        branch_id: &str,
        finality_epoch: u64,
    ) -> Result<CapabilityGrantV2, WorldError> {
        self.fixture_command_grant_with_nonce(
            agent_id,
            identity,
            world_id,
            branch_id,
            finality_epoch,
            format!("runtime-test-provider:{agent_id}:{branch_id}:{finality_epoch}"),
        )
    }

    fn fixture_command_grant_with_nonce(
        &self,
        agent_id: &str,
        identity: &CapabilityAgentIdentity,
        world_id: &str,
        branch_id: &str,
        finality_epoch: u64,
        grant_nonce: String,
    ) -> Result<CapabilityGrantV2, WorldError> {
        let issuer_key = fixture_signing_key(ISSUER_SEED);
        let finalized_receipt_id =
            format!("runtime-test-authority:{world_id}:{branch_id}:{finality_epoch}");
        let mut grant = CapabilityGrantV2 {
            grant_id: String::new(),
            grant_version: 2,
            subject: CapabilitySubject::Agent {
                agent_id: agent_id.to_string(),
                owner_binding: identity.owner_binding.clone(),
                generation: identity.generation,
            },
            audience: CapabilityAudience {
                world_id: world_id.to_string(),
                branch_id: branch_id.to_string(),
                finality_epoch,
                target_kind: "world".to_string(),
                target_id: None,
            },
            issuer: CapabilityIssuer {
                issuer_id: ISSUER_ID.to_string(),
                issuer_kind: "governance".to_string(),
                governance_epoch: finality_epoch,
                finalized_receipt_id,
                key_id: "governance-local-finality-key-1".to_string(),
                issuer_key_epoch: 1,
                authority_rotation_receipt_id: None,
                signature: String::new(),
            },
            scope: CapabilityScope {
                module_id: MODULE_ID.to_string(),
                module_version: MODULE_VERSION.to_string(),
                namespace: "provider".to_string(),
                object_kind: "command".to_string(),
                object_name: "observe".to_string(),
                operation: "execute".to_string(),
                entity_selector: None,
                resource_selector: None,
                max_payload_bytes: Some(1024),
                policy_class: Some("read-only".to_string()),
            },
            issued_at_tick: self.state.time,
            expires_at_tick: Some(self.state.time.saturating_add(100)),
            grant_nonce,
            parent_grant_id: None,
            delegation_depth: 0,
            revocation_epoch: 0,
            status: "verified".to_string(),
            canonical_body_hash: String::new(),
            issuance_signature: String::new(),
        };
        let body_hash = grant
            .canonical_body_hash()
            .map_err(|error| fixture_error(error.to_string()))?;
        grant.grant_id = body_hash.clone();
        grant.canonical_body_hash = body_hash;
        let signature = issuer_key.sign(
            grant
                .canonical_body_bytes()
                .map_err(|error| fixture_error(error.to_string()))?
                .as_slice(),
        );
        let signature = format!("ed25519:{}", hex::encode(signature.to_bytes()));
        grant.issuer.signature = signature.clone();
        grant.issuance_signature = signature;
        Ok(grant)
    }
}

fn fixture_module_artifact_identity() -> ModuleArtifactIdentity {
    let source_hash =
        crate::util::sha256_hex(b"runtime-provider-capability-fixture-source-v1");
    let build_manifest_hash =
        crate::util::sha256_hex(b"runtime-provider-capability-fixture-build-v1");
    let payload = ModuleArtifactIdentity::signing_payload_v1(
        MODULE_WASM_HASH,
        source_hash.as_str(),
        build_manifest_hash.as_str(),
        ISSUER_ID,
    );
    let signature = fixture_signing_key(ISSUER_SEED).sign(payload.as_slice());
    ModuleArtifactIdentity {
        source_hash,
        build_manifest_hash,
        signer_node_id: ISSUER_ID.to_string(),
        signature_scheme: ModuleArtifactIdentity::SIGNATURE_SCHEME_ED25519.to_string(),
        artifact_signature: format!(
            "{}{}",
            ModuleArtifactIdentity::SIGNATURE_PREFIX_ED25519_V1,
            hex::encode(signature.to_bytes())
        ),
    }
}

fn fixture_signing_key(seed_label: &[u8]) -> SigningKey {
    let seed = crate::util::sha256_hex(seed_label);
    let bytes = hex::decode(seed).expect("decode runtime fixture signing seed");
    SigningKey::from_bytes(
        &bytes
            .as_slice()
            .try_into()
            .expect("runtime fixture signing seed is 32 bytes"),
    )
}

fn fixture_error(reason: impl Into<String>) -> WorldError {
    WorldError::CapabilityAuthorizationDenied {
        reason: reason.into(),
    }
}

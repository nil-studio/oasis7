//! Explicit DevLocal provisioning for the W3 provider-backed chain.
//!
//! This is a local test initializer, not a trust-root import shim. It derives
//! the deterministic local issuer and finality signer material only after the
//! caller proves that the world uses the DevLocal storage profile and an
//! opt-in local authority mode. The resulting bundle still goes through the
//! ordinary proof-bearing provider bootstrap admission path.

use super::super::capability_authorization::{
    CapabilityAgentIdentity, CapabilityAuthorityFinalityBinding, CapabilityAuthorityFinalityProof,
    CapabilityAuthorityRecord,
};
use super::super::governance::GovernanceFinalityEpochSnapshot;
use super::super::{
    Action, ActionEnvelope, CausedBy, CognitionProvisioningReceiptV1,
    CognitionProvisioningRequestV1, DomainEvent, GovernanceFinalityCertificate, MainTokenConfig,
    MainTokenGenesisAllocationBucketState, MainTokenGenesisAllocationPlan, Manifest,
    ModuleAbiContract, ModuleActivation, ModuleArtifactIdentity, ModuleChangeSet, ModuleKind,
    ModuleLimits, ModuleManifest, ModuleRole, ProposalDecision, ProviderBackedBootstrapAuthorityV1,
    WorldError,
};
use super::World;
use super::governance::local_governance_finality_signing_keys;
use ed25519_dalek::Signer;
use oasis7_proto::storage_profile::StorageProfile;
use oasis7_wasm_abi::{CapabilityPresenter, ModuleCommandDeclaration, ModuleSchemaDeclarations};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[path = "local_test_provider_grant_lifecycle.rs"]
mod grant_lifecycle;
use grant_lifecycle::local_test_provider_grant;
pub use grant_lifecycle::{
    LOCAL_TEST_PROVIDER_GRANT_RENEWAL_THRESHOLD_TICKS, LOCAL_TEST_PROVIDER_GRANT_TTL_TICKS,
    LocalTestProviderGrantStatus,
};

pub const LOCAL_TEST_PROVIDER_MODULE_ID: &str = "module.runtime.local-test-provider";
pub const LOCAL_TEST_PROVIDER_MODULE_VERSION: &str = "1.0.0";
pub const LOCAL_TEST_PROVIDER_NAMESPACE: &str = "provider";
pub const LOCAL_TEST_PROVIDER_COMMAND: &str = "observe";
pub const LOCAL_TEST_PROVIDER_ISSUER_ID: &str = "governance.local.finality.signer.1";
pub const LOCAL_TEST_PROVIDER_KEY_ID: &str = "governance-local-finality-key-1";
pub const LOCAL_TEST_PROVIDER_MODULE_SCHEMA: &str = "provider.observe@1";

const LOCAL_W3_STARTER_MAIN_TOKEN_SUPPLY: u64 = 325;
const LOCAL_W3_STARTER_VESTING_BUCKET: &str = "dev_local_w3_starter_vesting";
const LOCAL_W3_STARTER_CLAIM_NONCE: u64 = 1;

/// Trust source selected by the caller. There is deliberately no remote
/// variant: remote authority provisioning remains an external governed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalTestProviderAuthorityMode {
    DevLocal,
}

/// Transport/session semantics are independent from the Runtime trust root.
/// The hosted public join session may supply the stable owner binding used by
/// this initializer, while the issuer remains local and DevLocal-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalTestProviderSessionMode {
    HostedPublicJoin,
    Loopback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalTestProviderAuthorityConfig {
    pub authority_mode: LocalTestProviderAuthorityMode,
    pub session_mode: LocalTestProviderSessionMode,
    pub storage_profile: StorageProfile,
    pub agent_id: String,
    /// For HostedPublicJoin this is the stable email-free player/session
    /// binding; it is never interpreted as an issuer or trust claim.
    pub owner_binding: String,
    pub owner_generation: u64,
    /// The current local chain finality marker. It must be supplied by the
    /// writer; this helper never invents a block hash.
    pub finality_block_hash: String,
    pub presenter: CapabilityPresenter,
    pub response_nonce: String,
    pub provision_id: String,
    pub authority_context: String,
    pub allowance: u64,
}

impl LocalTestProviderAuthorityConfig {
    pub fn hosted_public_join(
        agent_id: impl Into<String>,
        owner_binding: impl Into<String>,
        finality_block_hash: impl Into<String>,
    ) -> Self {
        let agent_id = agent_id.into();
        Self {
            authority_mode: LocalTestProviderAuthorityMode::DevLocal,
            session_mode: LocalTestProviderSessionMode::HostedPublicJoin,
            storage_profile: StorageProfile::DevLocal,
            owner_binding: owner_binding.into(),
            owner_generation: 1,
            finality_block_hash: finality_block_hash.into(),
            presenter: CapabilityPresenter {
                presenter_id: "provider-local-test".to_string(),
                presenter_kind: "provider".to_string(),
                session_id: Some(format!("hosted-public-join:{agent_id}")),
                attestation_ref: None,
            },
            response_nonce: format!("local-test-provider:{agent_id}"),
            provision_id: format!("local-test-provider:{agent_id}"),
            authority_context: "local-test-provider-authority".to_string(),
            allowance: 128,
            agent_id,
        }
    }
}

/// Packaged artifact and build metadata supplied by the canonical WASM build
/// suite. Source/build hashes are signed into the module manifest; this
/// initializer does not attest the builder image or claim production trust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTestProviderModuleArtifact {
    pub wasm_bytes: Vec<u8>,
    pub source_hash: String,
    pub build_manifest_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTestProviderProvisioning {
    pub bootstrap_authority: ProviderBackedBootstrapAuthorityV1,
    pub receipt: CognitionProvisioningReceiptV1,
    pub wasm_hash: String,
}

impl World {
    /// Install and activate the real local-test provider module before the
    /// cognition Runtime binding is established. Governed module publication
    /// changes the manifest hash, so this ordering preserves the binding's
    /// immutable manifest commitment.
    pub fn install_local_test_provider_module(
        &mut self,
        config: &LocalTestProviderAuthorityConfig,
        artifact: &LocalTestProviderModuleArtifact,
    ) -> Result<String, WorldError> {
        validate_local_test_config(self, config, artifact)?;
        if !self.cognition_runtime_is_unbound() {
            return Err(local_test_error(
                "local provider module must be installed before the Runtime binding",
            ));
        }
        let mut staged = self.clone();
        let persistence_dir = staged.persistence_dir.borrow().clone();
        *staged.persistence_dir.borrow_mut() = None;
        let wasm_hash = staged.prepare_local_test_provider_module(config, artifact)?;
        *staged.persistence_dir.borrow_mut() = persistence_dir;
        staged.persist_runtime_transaction_if_configured()?;
        *self = staged;
        Ok(wasm_hash)
    }

    /// Install the real local provider command and provision one agent using
    /// proof-bearing Runtime admission. This method is intentionally explicit
    /// so hosted session startup cannot accidentally become a trust bootstrap.
    /// Call [`Self::install_local_test_provider_module`] before binding
    /// cognition; the Runtime manifest binding is immutable after that point.
    pub fn initialize_local_test_provider_authority(
        &mut self,
        config: LocalTestProviderAuthorityConfig,
        artifact: LocalTestProviderModuleArtifact,
    ) -> Result<LocalTestProviderProvisioning, WorldError> {
        validate_local_test_config(self, &config, &artifact)?;

        let mut staged = self.clone();
        let persistence_dir = staged.persistence_dir.borrow().clone();
        *staged.persistence_dir.borrow_mut() = None;
        let result = staged.initialize_local_test_provider_authority_inner(config, artifact)?;
        *staged.persistence_dir.borrow_mut() = persistence_dir;
        staged.persist_runtime_transaction_if_configured()?;
        *self = staged;
        Ok(result)
    }

    /// Seed the bounded W3 DevLocal starter economy through the ordinary
    /// action evaluator and prepared monetary publication path. The fixture
    /// is deliberately fresh-only: callers must establish local authority
    /// first, and an existing token ledger is never topped up or rewritten.
    pub fn initialize_local_test_main_token_funding(
        &mut self,
        config: &LocalTestProviderAuthorityConfig,
    ) -> Result<(), WorldError> {
        validate_local_test_funding_config(self, config)?;
        if self.current_cognition_runtime_binding().is_err() {
            return Err(local_test_error(
                "local main-token funding requires an established Runtime binding",
            ));
        }
        if self
            .module_registry
            .active
            .get(LOCAL_TEST_PROVIDER_MODULE_ID)
            != Some(&LOCAL_TEST_PROVIDER_MODULE_VERSION.to_string())
        {
            return Err(local_test_error(
                "local main-token funding requires the active local provider module",
            ));
        }
        if !self
            .capability_revocation_state
            .authority_records
            .contains_key(LOCAL_TEST_PROVIDER_ISSUER_ID)
        {
            return Err(local_test_error(
                "local main-token funding requires admitted local provider authority",
            ));
        }
        let default_config = MainTokenConfig::default();
        if self.main_token_config() != &default_config
            || !self.state.main_token_genesis_buckets.is_empty()
            || !self.state.main_token_balances.is_empty()
            || !self.state.main_token_claim_nonces.is_empty()
            || !self.state.main_token_transfer_nonces.is_empty()
            || self.main_token_supply() != &Default::default()
        {
            return Err(local_test_error(
                "local main-token funding requires a pristine token ledger",
            ));
        }

        let mut staged = self.clone();
        let mut local_config = default_config;
        local_config.initial_supply = LOCAL_W3_STARTER_MAIN_TOKEN_SUPPLY;
        staged.set_main_token_config(local_config);
        staged.apply_local_test_main_token_action(Action::InitializeMainTokenGenesis {
            allocations: vec![MainTokenGenesisAllocationPlan {
                bucket_id: LOCAL_W3_STARTER_VESTING_BUCKET.to_string(),
                ratio_bps: 10_000,
                recipient: config.agent_id.clone(),
                cliff_epochs: 0,
                linear_unlock_epochs: 0,
                start_epoch: 0,
            }],
        })?;
        staged.apply_local_test_main_token_action(Action::ClaimMainTokenVesting {
            bucket_id: LOCAL_W3_STARTER_VESTING_BUCKET.to_string(),
            beneficiary: config.agent_id.clone(),
            nonce: LOCAL_W3_STARTER_CLAIM_NONCE,
        })?;
        *self = staged;
        Ok(())
    }

    /// Validate the immutable local funding provenance on an existing output.
    /// Current balances are intentionally left to normal replay and may have
    /// changed through later accepted gameplay actions.
    pub fn validate_local_test_main_token_funding(&self, agent_id: &str) -> Result<(), WorldError> {
        let expected_config = MainTokenConfig {
            initial_supply: LOCAL_W3_STARTER_MAIN_TOKEN_SUPPLY,
            ..MainTokenConfig::default()
        };
        if self.main_token_config() != &expected_config {
            return Err(local_test_error(
                "existing local world has no canonical W3 token configuration",
            ));
        }
        let funding_events = self
            .journal
            .events
            .iter()
            .filter_map(|event| match &event.body {
                super::super::WorldEventBody::Domain(
                    event @ (DomainEvent::MainTokenGenesisInitialized { .. }
                    | DomainEvent::MainTokenVestingClaimed { .. }),
                ) => Some(event),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !matches!(
            funding_events.as_slice(),
            [DomainEvent::MainTokenGenesisInitialized { total_supply, allocations },
                DomainEvent::MainTokenVestingClaimed {
                    bucket_id,
                    beneficiary,
                    amount,
                    nonce,
                }]
            if *total_supply == LOCAL_W3_STARTER_MAIN_TOKEN_SUPPLY
                && allocations.as_slice() == [MainTokenGenesisAllocationBucketState {
                    bucket_id: LOCAL_W3_STARTER_VESTING_BUCKET.to_string(),
                    ratio_bps: 10_000,
                    recipient: agent_id.to_string(),
                    cliff_epochs: 0,
                    linear_unlock_epochs: 0,
                    start_epoch: 0,
                    allocated_amount: LOCAL_W3_STARTER_MAIN_TOKEN_SUPPLY,
                    claimed_amount: 0,
                }]
                && bucket_id == LOCAL_W3_STARTER_VESTING_BUCKET
                && beneficiary == agent_id
                && *amount == LOCAL_W3_STARTER_MAIN_TOKEN_SUPPLY
                && *nonce == LOCAL_W3_STARTER_CLAIM_NONCE
        ) {
            return Err(local_test_error(
                "existing local world is missing canonical W3 token funding provenance",
            ));
        }
        let Some(bucket) = self.main_token_genesis_bucket(LOCAL_W3_STARTER_VESTING_BUCKET) else {
            return Err(local_test_error(
                "existing local world is missing the W3 vesting bucket",
            ));
        };
        if bucket.recipient != agent_id
            || bucket.ratio_bps != 10_000
            || bucket.allocated_amount != LOCAL_W3_STARTER_MAIN_TOKEN_SUPPLY
            || bucket.claimed_amount != LOCAL_W3_STARTER_MAIN_TOKEN_SUPPLY
            || bucket.cliff_epochs != 0
            || bucket.linear_unlock_epochs != 0
            || bucket.start_epoch != 0
            || self.main_token_supply().total_supply != LOCAL_W3_STARTER_MAIN_TOKEN_SUPPLY
            || self.main_token_supply().circulating_supply != LOCAL_W3_STARTER_MAIN_TOKEN_SUPPLY
            || self.main_token_supply().total_issued != 0
            || self.main_token_supply().total_burned != 0
            || self.main_token_last_claim_nonce(agent_id) != Some(LOCAL_W3_STARTER_CLAIM_NONCE)
        {
            return Err(local_test_error(
                "existing local world W3 token funding state does not match replayed provenance",
            ));
        }
        Ok(())
    }

    fn apply_local_test_main_token_action(&mut self, action: Action) -> Result<(), WorldError> {
        let action_id = self.allocate_next_action_id();
        let envelope = ActionEnvelope {
            committed_recipe_origin: None,
            id: action_id,
            action,
        };
        let body = self.action_to_event(&envelope)?;
        self.preflight_domain_event(&body)?;
        self.append_event(body, Some(CausedBy::Action(action_id)))?;
        Ok(())
    }

    fn initialize_local_test_provider_authority_inner(
        &mut self,
        config: LocalTestProviderAuthorityConfig,
        artifact: LocalTestProviderModuleArtifact,
    ) -> Result<LocalTestProviderProvisioning, WorldError> {
        let binding = self.current_cognition_runtime_binding()?;
        let has_existing_local_authority = self
            .capability_revocation_state
            .authority_records
            .contains_key(LOCAL_TEST_PROVIDER_ISSUER_ID);
        if binding.finality_epoch != self.current_governance_epoch()
            && !has_existing_local_authority
        {
            // A fresh authority must be certified in the current governance
            // epoch. Once that authority is installed, its finalized record
            // is immutable; startup-only grant renewal may reuse that
            // historical finality while the world advances through later
            // governance epochs.
            return Err(local_test_error(
                "local provider requires Runtime finality and governance epochs to match",
            ));
        }
        ensure_local_finality_snapshot(self, binding.finality_epoch)?;
        let identity = match self
            .capability_revocation_state
            .agent_identities
            .get(config.agent_id.as_str())
            .cloned()
        {
            Some(existing) if existing.owner_binding == config.owner_binding => {
                if existing.generation != config.owner_generation {
                    return Err(local_test_error(
                        "existing capability identity generation does not match local session",
                    ));
                }
                existing
            }
            Some(_) => {
                return Err(local_test_error(
                    "existing capability identity belongs to another owner",
                ));
            }
            None => {
                self.install_capability_agent_identity(
                    config.agent_id.as_str(),
                    config.owner_binding.clone(),
                    config.owner_generation,
                )?;
                CapabilityAgentIdentity {
                    owner_binding: config.owner_binding.clone(),
                    generation: config.owner_generation,
                }
            }
        };

        let wasm_hash = super::super::util::sha256_hex(&artifact.wasm_bytes);
        self.register_module_artifact(wasm_hash.clone(), &artifact.wasm_bytes)?;
        let module_manifest = local_test_provider_manifest(
            wasm_hash.as_str(),
            artifact.source_hash.as_str(),
            artifact.build_manifest_hash.as_str(),
            self.local_test_provider_module_identity(
                wasm_hash.as_str(),
                artifact.source_hash.as_str(),
                artifact.build_manifest_hash.as_str(),
            )?,
        )?;
        let record_key = super::super::ModuleRegistry::record_key(
            module_manifest.module_id.as_str(),
            module_manifest.version.as_str(),
        );
        if self
            .module_registry
            .records
            .get(&record_key)
            .map(|record| &record.manifest)
            != Some(&module_manifest)
        {
            return Err(local_test_error(
                "local provider module must be installed before the Runtime binding",
            ));
        }
        if self
            .module_registry
            .active
            .get(module_manifest.module_id.as_str())
            != Some(&module_manifest.version)
        {
            return Err(local_test_error(
                "local provider module must be active before authority provisioning",
            ));
        }
        let (authority_record, authority_proof) = if let Some(existing) = self
            .capability_revocation_state
            .authority_records
            .get(LOCAL_TEST_PROVIDER_ISSUER_ID)
            .cloned()
        {
            let expected = local_test_authority_record(
                binding.world_id.as_str(),
                binding.branch_id.as_str(),
                binding.finality_epoch,
                config.finality_block_hash.as_str(),
            );
            if existing != expected {
                return Err(local_test_error(
                    "existing local authority conflicts with the canonical DevLocal issuer",
                ));
            }
            let proof = self
                .capability_revocation_state
                .authority_finality_proofs
                .get(LOCAL_TEST_PROVIDER_ISSUER_ID)
                .cloned()
                .ok_or_else(|| local_test_error("local authority proof is missing"))?;
            validate_existing_local_authority(
                &existing,
                &proof,
                &binding,
                config.finality_block_hash.as_str(),
            )?;
            (existing, proof)
        } else {
            // The module lifecycle proposal carries `module_changes`, which
            // the governed apply path strips before recording the Applied
            // manifest hash. Authority finality must therefore be certified
            // by a second proposal over the already-applied manifest.
            let certificate =
                self.prepare_local_test_authority_certificate(config.agent_id.as_str())?;
            let authority = local_test_authority_record(
                binding.world_id.as_str(),
                binding.branch_id.as_str(),
                binding.finality_epoch,
                config.finality_block_hash.as_str(),
            );
            let proof = local_test_authority_proof(self, authority.clone(), certificate)?;
            (authority, proof)
        };

        if self
            .module_registry
            .active
            .get(LOCAL_TEST_PROVIDER_MODULE_ID)
            .is_none_or(|version| version != LOCAL_TEST_PROVIDER_MODULE_VERSION)
        {
            return Err(local_test_error("local provider module is not active"));
        }
        if !self
            .capability_revocation_state
            .authority_records
            .contains_key(LOCAL_TEST_PROVIDER_ISSUER_ID)
        {
            self.install_capability_authority_record_with_finality_proof(
                authority_record.clone(),
                authority_proof.clone(),
            )?;
        }

        let grant = local_test_provider_grant(
            &identity,
            config.agent_id.as_str(),
            &binding,
            &authority_record,
            self.state.time,
            config.response_nonce.as_str(),
        )?;
        let encoded_grant = serde_json::to_value(&grant)
            .map_err(|_| local_test_error("local provider grant cannot be encoded"))?;
        match self.capability_grants_v2.get(&grant.grant_id) {
            Some(existing) if existing == &encoded_grant => {}
            Some(_) => {
                return Err(local_test_error(
                    "existing local provider grant conflicts with the canonical grant",
                ));
            }
            None => self.register_capability_grant_v2(grant.clone())?,
        }
        let (_, derived_context) = self.capability_context_for_agent(
            config.agent_id.as_str(),
            config.presenter.clone(),
            config.response_nonce.clone(),
        )?;
        let invocation_context = self
            .capability_invocation_contexts
            .values()
            .find(|context| {
                context.grant_id == derived_context.grant_id
                    && context.response_nonce == derived_context.response_nonce
            })
            .cloned()
            .unwrap_or(derived_context);

        let request = CognitionProvisioningRequestV1::new(
            config.provision_id.clone(),
            identity.owner_binding.clone(),
            identity.owner_binding.clone(),
            identity.generation,
            binding.world_id.clone(),
            binding.branch_id.clone(),
            binding.reorg_epoch,
            config.allowance,
            config.authority_context.clone(),
        );
        let bootstrap_authority = ProviderBackedBootstrapAuthorityV1 {
            agent_id: config.agent_id,
            identity,
            authority_record,
            authority_finality_proof: authority_proof,
            grant,
            invocation_context,
            owner_binding: request.owner_binding.clone(),
            owner_generation: request.owner_generation,
            world_id: request.world_id.clone(),
            branch_id: request.branch_id.clone(),
            reorg_epoch: request.reorg_epoch,
            provision_id: request.provision_id.clone(),
            authority_context: request.authority_context.clone(),
            authority_digest: request.authority_digest.clone(),
            provisioning_digest: request.provisioning_digest.clone(),
            allowance: request.allowance,
        };
        let receipt = self.bootstrap_provider_backed_authority(bootstrap_authority.clone())?;
        // The authority record is finalized while the initial Runtime
        // binding is still the conservative `pending` wire projection. Align
        // the cognition wire status before any provider turn can capture it.
        self.promote_cognition_runtime_finality()?;
        Ok(LocalTestProviderProvisioning {
            bootstrap_authority,
            receipt,
            wasm_hash,
        })
    }

    fn ensure_local_test_provider_module(
        &mut self,
        manifest: &ModuleManifest,
        actor: &str,
    ) -> Result<(), WorldError> {
        let record_key = super::super::ModuleRegistry::record_key(
            manifest.module_id.as_str(),
            manifest.version.as_str(),
        );
        let mut changes = ModuleChangeSet::default();
        if let Some(existing) = self.module_registry.records.get(&record_key) {
            if existing.manifest != *manifest {
                return Err(local_test_error(
                    "local provider module manifest is immutable for this version",
                ));
            }
        } else {
            changes.register.push(manifest.clone());
        }
        if self
            .module_registry
            .active
            .get(manifest.module_id.as_str())
            .is_none_or(|version| version != manifest.version.as_str())
        {
            changes.activate.push(ModuleActivation {
                module_id: manifest.module_id.clone(),
                version: manifest.version.clone(),
            });
        }
        if self
            .module_registry
            .active
            .get(manifest.module_id.as_str())
            .is_some_and(|version| version == manifest.version.as_str())
        {
            return Ok(());
        }
        let mut content = match self.manifest.content.clone() {
            Value::Object(content) => content,
            _ => serde_json::Map::new(),
        };
        if !changes.register.is_empty() {
            content.insert("module_changes".to_string(), json!(changes));
        }
        content.insert(
            "local_test_provider_authority".to_string(),
            json!({"module_id": manifest.module_id, "version": manifest.version, "actor": actor}),
        );
        let proposal = self.propose_manifest_update(
            Manifest {
                version: self.manifest.version.saturating_add(1),
                content: Value::Object(content),
            },
            actor.to_string(),
        )?;
        self.shadow_proposal(proposal)?;
        self.approve_proposal(proposal, actor.to_string(), ProposalDecision::Approve)?;
        let certificate = self.build_local_finality_certificate(proposal)?;
        self.apply_proposal_with_finality(proposal, &certificate)?;
        Ok(())
    }

    fn prepare_local_test_provider_module(
        &mut self,
        config: &LocalTestProviderAuthorityConfig,
        artifact: &LocalTestProviderModuleArtifact,
    ) -> Result<String, WorldError> {
        if !self.state.agents.contains_key(&config.agent_id) {
            return Err(local_test_error(
                "local provider module installer requires a live agent",
            ));
        }
        let governance_epoch = self.current_governance_epoch();
        ensure_local_finality_snapshot(self, governance_epoch)?;
        let wasm_hash = super::super::util::sha256_hex(&artifact.wasm_bytes);
        self.register_module_artifact(wasm_hash.clone(), &artifact.wasm_bytes)?;
        let module_manifest = local_test_provider_manifest(
            wasm_hash.as_str(),
            artifact.source_hash.as_str(),
            artifact.build_manifest_hash.as_str(),
            self.local_test_provider_module_identity(
                wasm_hash.as_str(),
                artifact.source_hash.as_str(),
                artifact.build_manifest_hash.as_str(),
            )?,
        )?;
        self.ensure_local_test_provider_module(&module_manifest, config.agent_id.as_str())?;
        Ok(wasm_hash)
    }

    fn prepare_local_test_authority_certificate(
        &mut self,
        actor: &str,
    ) -> Result<GovernanceFinalityCertificate, WorldError> {
        let proposal = self.propose_manifest_update(self.manifest.clone(), actor.to_string())?;
        self.shadow_proposal(proposal)?;
        self.approve_proposal(proposal, actor.to_string(), ProposalDecision::Approve)?;
        let certificate = self.build_local_finality_certificate(proposal)?;
        self.apply_proposal_with_finality(proposal, &certificate)?;
        Ok(certificate)
    }

    fn local_test_provider_module_identity(
        &self,
        wasm_hash: &str,
        source_hash: &str,
        build_manifest_hash: &str,
    ) -> Result<ModuleArtifactIdentity, WorldError> {
        let signers = local_governance_finality_signing_keys();
        let (_, signer) = signers
            .into_iter()
            .find(|(node_id, _)| node_id == LOCAL_TEST_PROVIDER_ISSUER_ID)
            .ok_or_else(|| local_test_error("local issuer signing material is unavailable"))?;
        let payload = ModuleArtifactIdentity::signing_payload_v1(
            wasm_hash,
            source_hash,
            build_manifest_hash,
            LOCAL_TEST_PROVIDER_ISSUER_ID,
        );
        let signature = signer.sign(payload.as_slice());
        Ok(ModuleArtifactIdentity {
            source_hash: source_hash.to_string(),
            build_manifest_hash: build_manifest_hash.to_string(),
            signer_node_id: LOCAL_TEST_PROVIDER_ISSUER_ID.to_string(),
            signature_scheme: ModuleArtifactIdentity::SIGNATURE_SCHEME_ED25519.to_string(),
            artifact_signature: format!(
                "{}{}",
                ModuleArtifactIdentity::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(signature.to_bytes())
            ),
        })
    }
}

fn validate_local_test_config(
    world: &World,
    config: &LocalTestProviderAuthorityConfig,
    artifact: &LocalTestProviderModuleArtifact,
) -> Result<(), WorldError> {
    if config.authority_mode != LocalTestProviderAuthorityMode::DevLocal {
        return Err(local_test_error(
            "local provider authority mode is not DevLocal",
        ));
    }
    if config.storage_profile != StorageProfile::DevLocal {
        return Err(local_test_error(
            "local provider authority requires the DevLocal storage profile",
        ));
    }
    if world.release_security_policy().is_production_hardened()
        || !world.release_security_policy().allow_local_finality_signing
    {
        return Err(local_test_error(
            "local provider authority is disabled by the release security policy",
        ));
    }
    if world.governance_finality_signer_registry().is_some() {
        return Err(local_test_error(
            "local provider authority cannot replace an external finality signer registry",
        ));
    }
    for (node_id, signing_key) in local_governance_finality_signing_keys() {
        let expected = hex::encode(signing_key.verifying_key().to_bytes());
        if world.node_identity_public_key(node_id.as_str()) != Some(expected.as_str()) {
            return Err(local_test_error(format!(
                "local finality signer identity is not the canonical DevLocal key: {node_id}"
            )));
        }
    }
    if config.agent_id.trim().is_empty()
        || config.owner_binding.trim().is_empty()
        || config.finality_block_hash.trim().is_empty()
        || config.response_nonce.trim().is_empty()
        || config.provision_id.trim().is_empty()
        || config.authority_context.trim().is_empty()
        || config.owner_generation == 0
        || config.allowance == 0
    {
        return Err(local_test_error(
            "local provider authority config is incomplete",
        ));
    }
    config
        .presenter
        .validate()
        .map_err(|error| local_test_error(format!("local provider presenter: {error}")))?;
    if !is_sha256_hex(artifact.source_hash.as_str())
        || !is_sha256_hex(artifact.build_manifest_hash.as_str())
    {
        return Err(local_test_error(
            "local provider artifact source/build hashes must be SHA-256",
        ));
    }
    if artifact.wasm_bytes.is_empty() {
        return Err(local_test_error("local provider WASM artifact is empty"));
    }
    if let Some(binding) = world.cognition().get("runtime_binding") {
        let bound_hash = binding
            .get("finality_block_hash")
            .and_then(Value::as_str)
            .filter(|hash| !hash.trim().is_empty());
        if bound_hash != Some(config.finality_block_hash.as_str()) {
            return Err(local_test_error(
                "local provider finality marker does not match the live Runtime binding",
            ));
        }
    }
    Ok(())
}

fn validate_local_test_funding_config(
    world: &World,
    config: &LocalTestProviderAuthorityConfig,
) -> Result<(), WorldError> {
    if config.authority_mode != LocalTestProviderAuthorityMode::DevLocal {
        return Err(local_test_error(
            "local main-token funding mode is not DevLocal",
        ));
    }
    if config.storage_profile != StorageProfile::DevLocal {
        return Err(local_test_error(
            "local main-token funding requires the DevLocal storage profile",
        ));
    }
    if world.release_security_policy().is_production_hardened()
        || !world.release_security_policy().allow_local_finality_signing
    {
        return Err(local_test_error(
            "local main-token funding is disabled by the release security policy",
        ));
    }
    if world.governance_finality_signer_registry().is_some() {
        return Err(local_test_error(
            "local main-token funding cannot replace an external finality signer registry",
        ));
    }
    if config.agent_id.trim().is_empty() {
        return Err(local_test_error(
            "local main-token funding beneficiary cannot be empty",
        ));
    }
    for (node_id, signing_key) in local_governance_finality_signing_keys() {
        let expected = hex::encode(signing_key.verifying_key().to_bytes());
        if world.node_identity_public_key(node_id.as_str()) != Some(expected.as_str()) {
            return Err(local_test_error(format!(
                "local finality signer identity is not the canonical DevLocal key: {node_id}"
            )));
        }
    }
    Ok(())
}

fn ensure_local_finality_snapshot(world: &mut World, epoch_id: u64) -> Result<(), WorldError> {
    let signers = local_governance_finality_signing_keys();
    let signer_node_ids: Vec<String> = signers.iter().map(|(id, _)| id.clone()).collect();
    let validator_stakes = signer_node_ids
        .iter()
        .map(|id| (id.clone(), 100_u64))
        .collect::<BTreeMap<_, _>>();
    let snapshot = GovernanceFinalityEpochSnapshot {
        epoch_id,
        threshold_bps: 10_000,
        min_unique_signers: signer_node_ids.len() as u16,
        threshold: signer_node_ids.len() as u16,
        signer_node_ids,
        validator_stakes,
        ..GovernanceFinalityEpochSnapshot::default()
    };
    if let Some(existing) = world.governance_finality_epoch_snapshots().get(&epoch_id)
        && (existing.signer_node_ids != snapshot.signer_node_ids
            || existing.threshold_bps != snapshot.threshold_bps
            || existing.effective_min_unique_signers() != snapshot.effective_min_unique_signers())
    {
        return Err(local_test_error(
            "historical finality snapshot conflicts with canonical DevLocal signers",
        ));
    }
    world.set_governance_finality_epoch_snapshot(snapshot)
}

fn local_test_provider_manifest(
    wasm_hash: &str,
    source_hash: &str,
    build_manifest_hash: &str,
    identity: ModuleArtifactIdentity,
) -> Result<ModuleManifest, WorldError> {
    if identity.source_hash != source_hash || identity.build_manifest_hash != build_manifest_hash {
        return Err(local_test_error(
            "local provider artifact metadata does not match signed module identity",
        ));
    }
    let schema_hash = super::super::util::sha256_hex(LOCAL_TEST_PROVIDER_MODULE_SCHEMA.as_bytes());
    Ok(ModuleManifest {
        module_id: LOCAL_TEST_PROVIDER_MODULE_ID.to_string(),
        name: "Local Test Provider".to_string(),
        version: LOCAL_TEST_PROVIDER_MODULE_VERSION.to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::AgentInternal,
        wasm_hash: wasm_hash.to_string(),
        interface_version: "wasm-1".to_string(),
        exports: vec!["call".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        abi_contract: ModuleAbiContract {
            abi_version: Some(1),
            input_schema: Some("provider.input@1".to_string()),
            output_schema: Some("provider.output@1".to_string()),
            declarations: ModuleSchemaDeclarations {
                commands: vec![ModuleCommandDeclaration {
                    namespace: LOCAL_TEST_PROVIDER_NAMESPACE.to_string(),
                    name: LOCAL_TEST_PROVIDER_COMMAND.to_string(),
                    schema_version: 1,
                    schema_hash,
                    max_payload_bytes: 1024,
                }],
            },
            ..ModuleAbiContract::default()
        },
        artifact_identity: Some(identity),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 100_000,
            max_call_rate: 64,
            max_output_bytes: 4 * 1024,
            max_effects: 0,
            max_emits: 0,
        },
    })
}

fn local_test_authority_record(
    world_id: &str,
    branch_id: &str,
    finality_epoch: u64,
    finality_block_hash: &str,
) -> CapabilityAuthorityRecord {
    CapabilityAuthorityRecord {
        issuer_id: LOCAL_TEST_PROVIDER_ISSUER_ID.to_string(),
        issuer_kind: "governance".to_string(),
        key_id: LOCAL_TEST_PROVIDER_KEY_ID.to_string(),
        public_key_hex: local_signer_public_key(LOCAL_TEST_PROVIDER_ISSUER_ID),
        issuer_key_epoch: 1,
        governance_epoch: finality_epoch,
        finalized_receipt_id: format!(
            "local-test-authority:{world_id}:{branch_id}:{finality_epoch}"
        ),
        authority_rotation_receipt_id: None,
        world_id: world_id.to_string(),
        branch_id: branch_id.to_string(),
        finality_epoch,
        finality_block_hash: finality_block_hash.to_string(),
        finality_status: "finalized".to_string(),
        revocation_epoch: 0,
        revoked_grant_ids: BTreeSet::new(),
        superseded_by: BTreeMap::new(),
    }
}

fn local_test_authority_proof(
    world: &World,
    record: CapabilityAuthorityRecord,
    certificate: GovernanceFinalityCertificate,
) -> Result<CapabilityAuthorityFinalityProof, WorldError> {
    let binding =
        CapabilityAuthorityFinalityBinding::from_record(&record).map_err(local_test_error)?;
    let mut proof = CapabilityAuthorityFinalityProof {
        proof_version: CapabilityAuthorityFinalityProof::PROOF_VERSION_V1,
        certificate,
        binding,
        signatures: BTreeMap::new(),
    };
    let signers = local_governance_finality_signing_keys();
    for node_id in proof.certificate.signatures.keys() {
        let signer = signers
            .iter()
            .find(|(id, _)| id == node_id)
            .map(|(_, key)| key)
            .ok_or_else(|| local_test_error("local finality signer is unavailable"))?;
        let payload = proof
            .signing_payload_v1(node_id)
            .map_err(local_test_error)?;
        let signature = signer.sign(payload.as_slice());
        proof.signatures.insert(
            node_id.clone(),
            format!(
                "{}{}",
                CapabilityAuthorityFinalityProof::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(signature.to_bytes())
            ),
        );
    }
    world
        .governance_finality_epoch_snapshots()
        .get(&record.finality_epoch)
        .ok_or_else(|| local_test_error("local finality snapshot is missing"))?;
    Ok(proof)
}

fn validate_existing_local_authority(
    record: &CapabilityAuthorityRecord,
    proof: &CapabilityAuthorityFinalityProof,
    binding: &oasis7_agent_api::RuntimeBindingV1,
    finality_block_hash: &str,
) -> Result<(), WorldError> {
    if record.world_id != binding.world_id
        || record.branch_id != binding.branch_id
        || record.finality_epoch != binding.finality_epoch
        || record.finality_block_hash != finality_block_hash
        || record.finality_status != "finalized"
        || proof.binding
            != CapabilityAuthorityFinalityBinding::from_record(record).map_err(local_test_error)?
    {
        return Err(local_test_error(
            "existing local authority is bound to a different Runtime finality",
        ));
    }
    Ok(())
}

fn local_signer_public_key(node_id: &str) -> String {
    local_governance_finality_signing_keys()
        .into_iter()
        .find(|(id, _)| id == node_id)
        .map(|(_, key)| hex::encode(key.verifying_key().to_bytes()))
        .unwrap_or_default()
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn local_test_error(reason: impl Into<String>) -> WorldError {
    WorldError::CapabilityAuthorizationDenied {
        reason: format!("local test provider authority: {}", reason.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_agent_api::world_action::GeoPos;
    use crate::{Action, ReleaseSecurityPolicy};

    fn config() -> LocalTestProviderAuthorityConfig {
        let mut config = LocalTestProviderAuthorityConfig::hosted_public_join(
            "starter-agent-0",
            "local-test-player-0",
            "blake3:1111111111111111111111111111111111111111111111111111111111111111",
        );
        config
            .presenter
            .session_id
            .replace("hosted-session-local-test".to_string());
        config
    }

    fn artifact() -> LocalTestProviderModuleArtifact {
        LocalTestProviderModuleArtifact {
            wasm_bytes: b"not-a-wasm-artifact".to_vec(),
            source_hash: super::super::super::util::sha256_hex(
                b"oasis7-local-test-provider-module-source-v1",
            ),
            build_manifest_hash: super::super::super::util::sha256_hex(
                b"oasis7-local-test-provider-module-build-v1",
            ),
        }
    }

    #[test]
    fn hosted_session_is_accepted_as_session_semantics_but_production_world_is_rejected() {
        let mut world = World::new_production_hardened();
        world.submit_action(Action::RegisterAgent {
            agent_id: "starter-agent-0".to_string(),
            pos: GeoPos::new(0, 0, 0),
        });
        world.step().expect("register local test agent");
        world
            .bind_cognition_runtime(
                "live-formal-release-default",
                "main",
                0,
                Some(config().finality_block_hash.clone()),
                "pending",
                0,
            )
            .expect("bind runtime");
        let error = world
            .initialize_local_test_provider_authority(config(), artifact())
            .expect_err("production-hardened world must reject local authority");
        assert!(format!("{error:?}").contains("release security policy"));
    }

    #[test]
    fn release_policy_guard_does_not_accept_remote_storage_profile() {
        let world = World::new_with_release_security_policy(ReleaseSecurityPolicy::default());
        let mut config = config();
        config.storage_profile = StorageProfile::ReleaseDefault;
        let error = validate_local_test_config(&world, &config, &artifact())
            .expect_err("release profile must be rejected");
        assert!(format!("{error:?}").contains("DevLocal storage profile"));
    }

    #[test]
    fn local_main_token_funding_rejects_production_world() {
        let mut world = World::new_production_hardened();
        let error = world
            .initialize_local_test_main_token_funding(&config())
            .expect_err("production world must reject local funding");
        assert!(format!("{error:?}").contains("release security policy"));
    }

    #[test]
    fn local_provider_replay_reuses_receipt_without_duplicate_events() {
        let mut world = World::new();
        world.submit_action(Action::RegisterAgent {
            agent_id: "starter-agent-0".to_string(),
            pos: GeoPos::new(0, 0, 0),
        });
        world.step().expect("register local test agent");

        let config = config();
        let artifact = artifact();
        let wasm_hash = world
            .install_local_test_provider_module(&config, &artifact)
            .expect("install local test module before Runtime binding");
        world
            .bind_cognition_runtime(
                "live-formal-release-default",
                "main",
                0,
                Some(config.finality_block_hash.clone()),
                "pending",
                0,
            )
            .expect("bind Runtime");

        let first = world
            .initialize_local_test_provider_authority(config.clone(), artifact.clone())
            .expect("initialize local authority");
        assert_eq!(
            world
                .current_cognition_runtime_binding()
                .expect("promoted Runtime binding")
                .finality_status,
            "verified"
        );
        let event_count = world.journal.events.len();
        let replay = world
            .initialize_local_test_provider_authority(config, artifact)
            .expect("replay local authority");

        assert_eq!(first.wasm_hash, wasm_hash);
        assert_eq!(replay, first);
        assert_eq!(world.journal.events.len(), event_count);
    }
}

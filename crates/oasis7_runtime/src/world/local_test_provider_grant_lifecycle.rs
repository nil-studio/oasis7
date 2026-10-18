use super::super::governance::local_governance_finality_signing_keys;
use super::{
    CapabilityAgentIdentity, CapabilityAuthorityRecord, CognitionProvisioningRequestV1,
    LOCAL_TEST_PROVIDER_COMMAND, LOCAL_TEST_PROVIDER_ISSUER_ID, LOCAL_TEST_PROVIDER_MODULE_ID,
    LOCAL_TEST_PROVIDER_MODULE_VERSION, LOCAL_TEST_PROVIDER_NAMESPACE,
    LocalTestProviderAuthorityConfig, LocalTestProviderModuleArtifact,
    ProviderBackedBootstrapAuthorityV1, World, WorldError, local_test_error,
    local_test_provider_manifest, validate_existing_local_authority, validate_local_test_config,
};
use ed25519_dalek::Signer;
use oasis7_wasm_abi::{
    CapabilityAudience, CapabilityGrantV2, CapabilityIssuer, CapabilityScope, CapabilitySubject,
};

pub const LOCAL_TEST_PROVIDER_GRANT_TTL_TICKS: u64 = 300;
pub const LOCAL_TEST_PROVIDER_GRANT_RENEWAL_THRESHOLD_TICKS: u64 = 240;

/// Startup-only status for the explicit DevLocal local-test-provider grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTestProviderGrantStatus {
    pub remaining_ticks: u64,
}

impl World {
    /// Validate the persisted local-test-provider authority before startup
    /// decides whether it can reuse the bundle or must issue a replacement.
    /// Expired grants are valid historical evidence here; the caller uses the
    /// returned remaining lifetime to select the renewal path.
    pub fn local_test_provider_grant_status(
        &self,
        config: &LocalTestProviderAuthorityConfig,
        artifact: &LocalTestProviderModuleArtifact,
        authority: &ProviderBackedBootstrapAuthorityV1,
    ) -> Result<Option<LocalTestProviderGrantStatus>, WorldError> {
        validate_local_test_config(self, config, artifact)?;
        let binding = self.current_cognition_runtime_binding()?;
        if authority.agent_id != config.agent_id
            || authority.owner_binding != config.owner_binding
            || authority.owner_generation != config.owner_generation
            || authority.identity.owner_binding != config.owner_binding
            || authority.identity.generation != config.owner_generation
            || authority.world_id != binding.world_id
            || authority.branch_id != binding.branch_id
            || authority.reorg_epoch != binding.reorg_epoch
        {
            return Err(local_test_error(
                "local test authority bundle does not match the requested Runtime binding",
            ));
        }
        if !self.state.agents.contains_key(config.agent_id.as_str()) {
            return Err(local_test_error(
                "local test authority requires a live agent",
            ));
        }
        let identity = self
            .capability_revocation_state
            .agent_identities
            .get(config.agent_id.as_str())
            .ok_or_else(|| local_test_error("local test capability identity is missing"))?;
        if identity != &authority.identity {
            return Err(local_test_error(
                "local test capability identity does not match the authority bundle",
            ));
        }

        let wasm_hash = crate::util::sha256_hex(&artifact.wasm_bytes);
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
            || self
                .module_registry
                .active
                .get(module_manifest.module_id.as_str())
                != Some(&module_manifest.version)
        {
            return Err(local_test_error(
                "local test provider module does not match the requested artifact",
            ));
        }

        let expected_authority = super::local_test_authority_record(
            binding.world_id.as_str(),
            binding.branch_id.as_str(),
            binding.finality_epoch,
            config.finality_block_hash.as_str(),
        );
        if authority.authority_record != expected_authority
            || self
                .capability_revocation_state
                .authority_records
                .get(LOCAL_TEST_PROVIDER_ISSUER_ID)
                != Some(&expected_authority)
        {
            return Err(local_test_error(
                "local test authority record does not match the canonical DevLocal authority",
            ));
        }
        let stored_proof = self
            .capability_revocation_state
            .authority_finality_proofs
            .get(LOCAL_TEST_PROVIDER_ISSUER_ID)
            .ok_or_else(|| local_test_error("local test authority proof is missing"))?;
        validate_existing_local_authority(
            &expected_authority,
            stored_proof,
            &binding,
            config.finality_block_hash.as_str(),
        )?;
        if stored_proof != &authority.authority_finality_proof {
            return Err(local_test_error(
                "local test authority proof does not match the persisted bundle",
            ));
        }

        validate_local_test_provider_grant(
            self,
            &authority.grant,
            identity,
            config,
            &binding,
            &expected_authority,
        )?;
        let encoded_grant = serde_json::to_value(&authority.grant)
            .map_err(|_| local_test_error("local test provider grant cannot be encoded"))?;
        let Some(stored_grant) = self.capability_grants_v2.get(&authority.grant.grant_id) else {
            let local_subject_grant_exists = self
                .capability_grants_v2
                .values()
                .filter_map(|encoded| {
                    serde_json::from_value::<CapabilityGrantV2>(encoded.clone()).ok()
                })
                .any(|grant| {
                    grant.subject
                        == (CapabilitySubject::Agent {
                            agent_id: config.agent_id.clone(),
                            owner_binding: config.owner_binding.clone(),
                            generation: config.owner_generation,
                        })
                });
            if local_subject_grant_exists {
                return Err(local_test_error(
                    "local test authority bundle grant does not match durable grant state",
                ));
            }
            return Ok(None);
        };
        if stored_grant != &encoded_grant {
            return Err(local_test_error(
                "local test provider grant is immutable and differs from the authority bundle",
            ));
        }
        let stored_context = self
            .capability_invocation_contexts
            .values()
            .find(|context| context.grant_id == authority.grant.grant_id)
            .ok_or_else(|| local_test_error("local test invocation context is missing"))?;
        if stored_context != &authority.invocation_context {
            return Err(local_test_error(
                "local test invocation context does not match the authority bundle",
            ));
        }
        let economy = self.cognition_economy()?;
        let provision = economy
            .provisions
            .get(authority.provision_id.as_str())
            .ok_or_else(|| local_test_error("local test cognition provisioning is missing"))?;
        let expected_request = CognitionProvisioningRequestV1::new(
            authority.provision_id.clone(),
            config.owner_binding.clone(),
            config.owner_binding.clone(),
            config.owner_generation,
            binding.world_id.clone(),
            binding.branch_id.clone(),
            binding.reorg_epoch,
            config.allowance,
            config.authority_context.clone(),
        );
        if provision.request != expected_request {
            return Err(local_test_error(
                "local test cognition provisioning does not match the authority bundle",
            ));
        }
        let expiry = authority
            .grant
            .expires_at_tick
            .ok_or_else(|| local_test_error("local test provider grant expiry is missing"))?;
        Ok(Some(LocalTestProviderGrantStatus {
            remaining_ticks: expiry.saturating_sub(self.state.time),
        }))
    }
}

fn validate_local_test_provider_grant(
    world: &World,
    grant: &CapabilityGrantV2,
    identity: &CapabilityAgentIdentity,
    config: &LocalTestProviderAuthorityConfig,
    binding: &oasis7_agent_api::RuntimeBindingV1,
    authority: &CapabilityAuthorityRecord,
) -> Result<(), WorldError> {
    grant
        .validate()
        .map_err(|error| local_test_error(format!("local provider grant: {error}")))?;
    if !grant
        .body_hash_matches()
        .map_err(|error| local_test_error(format!("local provider grant body hash: {error}")))?
        || grant
            .expected_grant_id()
            .map_err(|error| local_test_error(format!("local provider grant id hash: {error}")))?
            != grant.grant_id
    {
        return Err(local_test_error(
            "local provider grant canonical identity mismatch",
        ));
    }
    world.verify_issuer(grant)?;
    world.verify_live_revocation(grant)?;
    world.verify_parent_chain(grant)?;
    if grant.subject
        != (CapabilitySubject::Agent {
            agent_id: config.agent_id.clone(),
            owner_binding: identity.owner_binding.clone(),
            generation: identity.generation,
        })
        || grant.audience
            != (CapabilityAudience {
                world_id: binding.world_id.clone(),
                branch_id: binding.branch_id.clone(),
                finality_epoch: binding.finality_epoch,
                target_kind: "world".to_string(),
                target_id: None,
            })
    {
        return Err(local_test_error(
            "local provider grant subject or audience does not match the local binding",
        ));
    }
    if grant.issuer.issuer_id != authority.issuer_id
        || grant.issuer.issuer_kind != authority.issuer_kind
        || grant.issuer.governance_epoch != authority.governance_epoch
        || grant.issuer.finalized_receipt_id != authority.finalized_receipt_id
        || grant.issuer.key_id != authority.key_id
        || grant.issuer.issuer_key_epoch != authority.issuer_key_epoch
        || grant.issuer.authority_rotation_receipt_id != authority.authority_rotation_receipt_id
    {
        return Err(local_test_error(
            "local provider grant issuer does not match the local authority",
        ));
    }
    if grant.scope
        != (CapabilityScope {
            module_id: LOCAL_TEST_PROVIDER_MODULE_ID.to_string(),
            module_version: LOCAL_TEST_PROVIDER_MODULE_VERSION.to_string(),
            namespace: LOCAL_TEST_PROVIDER_NAMESPACE.to_string(),
            object_kind: "command".to_string(),
            object_name: LOCAL_TEST_PROVIDER_COMMAND.to_string(),
            operation: "execute".to_string(),
            entity_selector: None,
            resource_selector: None,
            max_payload_bytes: Some(1024),
            policy_class: Some("read-only".to_string()),
        })
    {
        return Err(local_test_error(
            "local provider grant scope does not match the canonical read-only command",
        ));
    }
    if grant.issued_at_tick > world.state.time
        || grant.expires_at_tick.is_none()
        || grant.grant_nonce != config.response_nonce
        || grant.parent_grant_id.is_some()
        || grant.delegation_depth != 0
        || grant.revocation_epoch != authority.revocation_epoch
        || grant.status != "verified"
    {
        return Err(local_test_error(
            "local provider grant lifetime or nonce binding is invalid",
        ));
    }
    Ok(())
}

pub(super) fn local_test_provider_grant(
    identity: &CapabilityAgentIdentity,
    agent_id: &str,
    binding: &oasis7_agent_api::RuntimeBindingV1,
    authority: &CapabilityAuthorityRecord,
    issued_at_tick: u64,
    grant_nonce: &str,
) -> Result<CapabilityGrantV2, WorldError> {
    let mut grant = CapabilityGrantV2 {
        grant_id: String::new(),
        grant_version: 2,
        subject: CapabilitySubject::Agent {
            agent_id: agent_id.to_string(),
            owner_binding: identity.owner_binding.clone(),
            generation: identity.generation,
        },
        audience: CapabilityAudience {
            world_id: binding.world_id.clone(),
            branch_id: binding.branch_id.clone(),
            finality_epoch: binding.finality_epoch,
            target_kind: "world".to_string(),
            target_id: None,
        },
        issuer: CapabilityIssuer {
            issuer_id: authority.issuer_id.clone(),
            issuer_kind: authority.issuer_kind.clone(),
            governance_epoch: authority.governance_epoch,
            finalized_receipt_id: authority.finalized_receipt_id.clone(),
            key_id: authority.key_id.clone(),
            issuer_key_epoch: authority.issuer_key_epoch,
            authority_rotation_receipt_id: authority.authority_rotation_receipt_id.clone(),
            signature: String::new(),
        },
        scope: CapabilityScope {
            module_id: LOCAL_TEST_PROVIDER_MODULE_ID.to_string(),
            module_version: LOCAL_TEST_PROVIDER_MODULE_VERSION.to_string(),
            namespace: LOCAL_TEST_PROVIDER_NAMESPACE.to_string(),
            object_kind: "command".to_string(),
            object_name: LOCAL_TEST_PROVIDER_COMMAND.to_string(),
            operation: "execute".to_string(),
            entity_selector: None,
            resource_selector: None,
            max_payload_bytes: Some(1024),
            policy_class: Some("read-only".to_string()),
        },
        issued_at_tick,
        expires_at_tick: Some(issued_at_tick.saturating_add(LOCAL_TEST_PROVIDER_GRANT_TTL_TICKS)),
        grant_nonce: grant_nonce.to_string(),
        parent_grant_id: None,
        delegation_depth: 0,
        revocation_epoch: authority.revocation_epoch,
        status: "verified".to_string(),
        canonical_body_hash: String::new(),
        issuance_signature: String::new(),
    };
    let body_hash = grant
        .canonical_body_hash()
        .map_err(|error| local_test_error(error.to_string()))?;
    grant.grant_id = body_hash.clone();
    grant.canonical_body_hash = body_hash;
    let signer = local_governance_finality_signing_keys()
        .into_iter()
        .find(|(node_id, _)| *node_id == authority.issuer_id)
        .map(|(_, key)| key)
        .ok_or_else(|| local_test_error("local issuer signing material is unavailable"))?;
    let signature = signer.sign(
        grant
            .canonical_body_bytes()
            .map_err(|error| local_test_error(error.to_string()))?
            .as_slice(),
    );
    let signature = format!("ed25519:{}", hex::encode(signature.to_bytes()));
    grant.issuer.signature = signature.clone();
    grant.issuance_signature = signature;
    Ok(grant)
}

//! Atomic bootstrap for a proof-bearing ProviderBacked cognition authority.
//!
//! The viewer may carry this bundle across a process boundary, but Runtime is
//! the only component allowed to validate and install its authority records or
//! create the cognition allowance.  Every field that can bind a provider turn
//! to a different owner, world, branch, generation, or replay identity is
//! checked before the final provisioning transaction is published.

use super::super::capability_authorization::{
    CapabilityAgentIdentity, CapabilityAuthorityFinalityProof, CapabilityAuthorityRecord,
    CapabilityInvocationContext,
};
use super::super::error::WorldError;
use super::World;
use super::cognition_economy::{CognitionProvisioningReceiptV1, CognitionProvisioningRequestV1};
use oasis7_wasm_abi::{CapabilityGrantV2, CapabilitySubject};
use serde::{Deserialize, Serialize};

/// Pre-verified authority input for one ProviderBacked agent.
///
/// The bundle contains signed/finalized capability evidence and an explicit
/// provisioning digest. Runtime still revalidates every record against its
/// live governance, module, capability, and cognition bindings before any
/// economy mutation. No owner, grant, authority, or allowance is synthesized
/// when this input is absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderBackedBootstrapAuthorityV1 {
    pub agent_id: String,
    pub identity: CapabilityAgentIdentity,
    pub authority_record: CapabilityAuthorityRecord,
    pub authority_finality_proof: CapabilityAuthorityFinalityProof,
    pub grant: CapabilityGrantV2,
    pub invocation_context: CapabilityInvocationContext,
    pub owner_binding: String,
    pub owner_generation: u64,
    pub world_id: String,
    pub branch_id: String,
    pub reorg_epoch: u64,
    pub provision_id: String,
    pub authority_context: String,
    pub authority_digest: String,
    pub provisioning_digest: String,
    pub allowance: u64,
}

impl World {
    /// Atomically install a proof-bearing provider authority and provision its
    /// fixed-unit cognition allowance. Exact replay returns the existing
    /// receipt without appending duplicate authority or economy evidence.
    pub fn bootstrap_provider_backed_authority(
        &mut self,
        input: ProviderBackedBootstrapAuthorityV1,
    ) -> Result<CognitionProvisioningReceiptV1, WorldError> {
        let mut receipts =
            self.bootstrap_provider_backed_authorities(std::slice::from_ref(&input))?;
        Ok(receipts
            .pop()
            .expect("single provider bootstrap always returns one receipt"))
    }

    /// Atomically install a set of proof-bearing provider authorities. The
    /// staged world has persistence disabled until every input validates and
    /// provisions, so a later failure cannot leave an earlier bundle on disk.
    pub fn bootstrap_provider_backed_authorities(
        &mut self,
        inputs: &[ProviderBackedBootstrapAuthorityV1],
    ) -> Result<Vec<CognitionProvisioningReceiptV1>, WorldError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let mut staged = self.clone();
        let persistence_dir = staged.persistence_dir.borrow().clone();
        *staged.persistence_dir.borrow_mut() = None;
        let mut receipts = Vec::with_capacity(inputs.len());
        for input in inputs {
            receipts.push(staged.bootstrap_provider_backed_authority_inner(input)?);
        }
        *staged.persistence_dir.borrow_mut() = persistence_dir;
        staged.persist_runtime_transaction_if_configured()?;
        *self = staged;
        Ok(receipts)
    }

    fn bootstrap_provider_backed_authority_inner(
        &mut self,
        input: &ProviderBackedBootstrapAuthorityV1,
    ) -> Result<CognitionProvisioningReceiptV1, WorldError> {
        self.validate_provider_backed_bootstrap_input(input)?;

        self.install_capability_agent_identity(
            input.agent_id.as_str(),
            input.identity.owner_binding.clone(),
            input.identity.generation,
        )?;

        let authority_is_replayed = self
            .capability_revocation_state
            .authority_records
            .get(&input.authority_record.issuer_id)
            == Some(&input.authority_record)
            && self
                .capability_revocation_state
                .authority_finality_proofs
                .get(&input.authority_record.issuer_id)
                == Some(&input.authority_finality_proof);
        if !authority_is_replayed {
            self.install_capability_authority_record_with_finality_proof(
                input.authority_record.clone(),
                input.authority_finality_proof.clone(),
            )?;
        }

        let encoded_grant = serde_json::to_value(&input.grant)
            .map_err(|_| bootstrap_denied("provider bootstrap grant cannot be encoded"))?;
        match self.capability_grants_v2.get(&input.grant.grant_id) {
            Some(existing) if existing == &encoded_grant => {}
            Some(_) => return Err(bootstrap_denied("provider bootstrap grant is immutable")),
            None => self.register_capability_grant_v2(input.grant.clone())?,
        }

        // The supplied context is checked against a context derived from the
        // now-installed identity, grant, module declaration, and live finality
        // audience. A serialized context is never trusted on its own.
        let (_, derived_context) = self.capability_context_for_agent(
            input.agent_id.as_str(),
            input.invocation_context.presenter.clone(),
            input.invocation_context.response_nonce.clone(),
        )?;
        if derived_context.grant_id != input.grant.grant_id {
            return Err(bootstrap_denied(
                "provider bootstrap derived invocation context grant does not match supplied grant",
            ));
        }
        // Installing identity/authority/grant evidence appends its own
        // journal events, so the supplied catalog snapshot id may describe
        // the pre-bootstrap journal head. The authority-bearing fields are
        // compared above. An existing context for the same grant/nonce is
        // immutable; retain it when only its historical catalog snapshot
        // differs from the freshly derived post-install snapshot.
        let existing_context = self
            .capability_invocation_contexts
            .values()
            .find(|context| {
                context.grant_id == derived_context.grant_id
                    && context.response_nonce == derived_context.response_nonce
            })
            .cloned();
        if let Some(existing_context) = existing_context {
            if existing_context.subject != derived_context.subject
                || existing_context.presenter != derived_context.presenter
                || existing_context.audience != derived_context.audience
                || existing_context.module_id != derived_context.module_id
                || existing_context.module_version != derived_context.module_version
            {
                return Err(bootstrap_denied(
                    "provider bootstrap invocation context is bound to different authority",
                ));
            }
        } else {
            self.install_capability_invocation_context(derived_context)?;
        }

        // Keep provisioning last. If any authority/context check fails, no
        // economy state is persisted. The request digest is recomputed from
        // Runtime-derived identity and binding fields before this call.
        let receipt = self.provision_cognition_for_agent(
            input.agent_id.as_str(),
            input.provision_id.clone(),
            input.authority_context.clone(),
            input.allowance,
        )?;
        if receipt.request.owner_binding != input.owner_binding
            || receipt.request.owner_generation != input.owner_generation
            || receipt.request.world_id != input.world_id
            || receipt.request.branch_id != input.branch_id
            || receipt.request.reorg_epoch != input.reorg_epoch
            || receipt.request.authority_digest != input.authority_digest
            || receipt.request.provisioning_digest != input.provisioning_digest
        {
            return Err(bootstrap_denied(
                "provider bootstrap provisioning receipt identity mismatch",
            ));
        }

        Ok(receipt)
    }

    fn validate_provider_backed_bootstrap_input(
        &self,
        input: &ProviderBackedBootstrapAuthorityV1,
    ) -> Result<(), WorldError> {
        let binding = self.current_cognition_runtime_binding()?;
        if input.agent_id.trim().is_empty()
            || input.owner_binding.trim().is_empty()
            || input.owner_generation == 0
            || input.identity.owner_binding != input.owner_binding
            || input.identity.generation != input.owner_generation
            || input.world_id != binding.world_id
            || input.branch_id != binding.branch_id
            || input.reorg_epoch != binding.reorg_epoch
        {
            return Err(bootstrap_denied(
                "provider bootstrap identity does not match live Runtime binding",
            ));
        }
        if !self.state.agents.contains_key(&input.agent_id) {
            return Err(bootstrap_denied("provider bootstrap requires a live agent"));
        }

        match &input.grant.subject {
            CapabilitySubject::Agent {
                agent_id,
                owner_binding,
                generation,
            } if agent_id == &input.agent_id
                && owner_binding == &input.owner_binding
                && *generation == input.owner_generation => {}
            _ => {
                return Err(bootstrap_denied(
                    "provider bootstrap grant subject does not match identity",
                ));
            }
        }
        if input.grant.audience.world_id != input.world_id
            || input.grant.audience.branch_id != input.branch_id
            || input.grant.audience.finality_epoch != binding.finality_epoch
        {
            return Err(bootstrap_denied(
                "provider bootstrap grant audience does not match Runtime binding",
            ));
        }
        if input.authority_record.world_id != input.world_id
            || input.authority_record.branch_id != input.branch_id
            || input.authority_record.finality_epoch != binding.finality_epoch
            || input.authority_record.finality_status != "finalized"
            || input.authority_record.issuer_id != input.grant.issuer.issuer_id
        {
            return Err(bootstrap_denied(
                "provider bootstrap authority record does not match grant binding",
            ));
        }
        if input.invocation_context.grant_id != input.grant.grant_id
            || input.invocation_context.subject != input.grant.subject
            || input.invocation_context.audience != input.grant.audience
            || input.invocation_context.module_id != input.grant.scope.module_id
            || input.invocation_context.module_version != input.grant.scope.module_version
        {
            return Err(bootstrap_denied(
                "provider bootstrap invocation context identity mismatch",
            ));
        }

        let expected_request = CognitionProvisioningRequestV1::new(
            input.provision_id.clone(),
            input.owner_binding.clone(),
            input.owner_binding.clone(),
            input.owner_generation,
            input.world_id.clone(),
            input.branch_id.clone(),
            input.reorg_epoch,
            input.allowance,
            input.authority_context.clone(),
        );
        expected_request
            .validate()
            .map_err(|error| bootstrap_denied(format!("provider bootstrap request: {error}")))?;
        if expected_request.authority_digest != input.authority_digest
            || expected_request.provisioning_digest != input.provisioning_digest
        {
            return Err(bootstrap_denied(
                "provider bootstrap provisioning digest does not match request",
            ));
        }
        Ok(())
    }
}

fn bootstrap_denied(reason: impl Into<String>) -> WorldError {
    WorldError::CapabilityAuthorizationDenied {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use oasis7_agent_api::world_action::GeoPos;
    use crate::{Action, ChainResourceDerivationContext, World};
    use oasis7_wasm_abi::CapabilityPresenter;
    use std::fs;

    fn fixture_world() -> World {
        let mut world = World::new();
        world.submit_action(Action::RegisterAgent {
            agent_id: "agent-a".to_string(),
            pos: GeoPos::new(0, 0, 0),
        });
        world.step().expect("register fixture agent");
        world
            .bind_cognition_runtime("provision-world", "main", 0, None, "pending", 0)
            .expect("bind fixture cognition");
        world
            .install_test_provider_capability_fixture_without_cognition_balance("agent-a")
            .expect("install fixture authority");
        world
    }

    #[test]
    fn provider_bootstrap_is_exactly_replayable_across_restart() {
        let mut world = fixture_world();
        let input = world
            .test_provider_backed_bootstrap_authority(
                "agent-a",
                "provider-bootstrap-a",
                "provider-bootstrap-authority",
                7,
            )
            .expect("build provider bootstrap input");
        let first = world
            .bootstrap_provider_backed_authority(input.clone())
            .expect("bootstrap provider authority");
        assert_eq!(first.request.account_id, input.owner_binding);
        assert_eq!(
            world
                .cognition_economy()
                .expect("read provisioned economy")
                .available_balance(input.owner_binding.as_str(), "cognition_units"),
            input.allowance
        );
        let first_economy = world.cognition_economy().expect("first economy");
        assert_eq!(first_economy.provision_journal.len(), 1);

        let replay = world
            .bootstrap_provider_backed_authority(input.clone())
            .expect("replay provider authority");
        assert_eq!(replay, first);
        assert_eq!(
            world.cognition_economy().expect("replayed economy"),
            first_economy
        );

        let dir = std::env::temp_dir().join(format!(
            "oasis7-provider-bootstrap-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_dir_all(&dir);
        world
            .save_to_dir_with_chain_resource_context(
                &dir,
                ChainResourceDerivationContext {
                    world_id: "provision-world",
                    chain_id: "runtime-chain",
                    genesis_ref: None,
                    created_at_height: 0,
                    manifest_height: 0,
                    commit_block_hash: None,
                    tick: world.state().time,
                },
                "provider-bootstrap-config",
                "provider-bootstrap-generation",
            )
            .expect("save provider bootstrap world");
        let mut restored = World::load_from_dir(&dir).expect("restore provider bootstrap world");
        let restored_before = restored.cognition_economy().expect("restored economy");
        let restored_replay = restored
            .bootstrap_provider_backed_authority(input)
            .expect("replay restored provider authority");
        assert_eq!(restored_replay, first);
        assert_eq!(
            restored
                .cognition_economy()
                .expect("restored replay economy"),
            restored_before
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn provider_bootstrap_rejects_binding_mismatch_before_mutation() {
        let mut world = fixture_world();
        let mut input = world
            .test_provider_backed_bootstrap_authority(
                "agent-a",
                "provider-bootstrap-mismatch",
                "provider-bootstrap-authority",
                7,
            )
            .expect("build provider bootstrap input");
        input.owner_binding = "wrong-owner".to_string();
        let before = world.snapshot();

        let error = world
            .bootstrap_provider_backed_authority(input)
            .expect_err("mismatched owner must fail closed");
        assert!(format!("{error:?}").contains("does not match live Runtime binding"));
        assert_eq!(world.snapshot(), before);
    }

    #[test]
    fn provider_bootstrap_rejects_multi_grant_context_mismatch_before_provisioning() {
        let mut world = fixture_world();
        let primary = world
            .test_provider_backed_bootstrap_authority(
                "agent-a",
                "provider-bootstrap-multi-grant",
                "provider-bootstrap-authority",
                7,
            )
            .expect("build primary provider bootstrap input");
        let secondary_grant = world
            .install_test_provider_additional_capability_grant("agent-a", "secondary")
            .expect("install secondary provider grant");
        let (_, selected_context) = world
            .capability_context_for_agent(
                "agent-a",
                CapabilityPresenter {
                    presenter_id: "runtime-test-provider:agent-a".to_string(),
                    presenter_kind: "provider".to_string(),
                    session_id: Some("runtime-test-session:agent-a".to_string()),
                    attestation_ref: None,
                },
                "provider-bootstrap-multi-grant-selection",
            )
            .expect("derive canonical selected grant");

        let mut mismatched = primary.clone();
        if selected_context.grant_id == primary.grant.grant_id {
            mismatched.grant = secondary_grant;
            mismatched.invocation_context.grant_id = mismatched.grant.grant_id.clone();
            mismatched.invocation_context.subject = mismatched.grant.subject.clone();
            mismatched.invocation_context.audience = mismatched.grant.audience.clone();
            mismatched.invocation_context.module_id = mismatched.grant.scope.module_id.clone();
            mismatched.invocation_context.module_version =
                mismatched.grant.scope.module_version.clone();
        }
        assert_ne!(mismatched.grant.grant_id, selected_context.grant_id);
        let before = world.snapshot();

        let error = world
            .bootstrap_provider_backed_authority(mismatched)
            .expect_err("ambiguous multi-grant selection must fail closed");
        assert!(format!("{error:?}").contains("derived invocation context grant"));
        assert!(
            world
                .cognition_economy()
                .expect("read unchanged economy")
                .provision_journal
                .is_empty()
        );
        assert_eq!(world.snapshot(), before);
    }

    #[test]
    fn provider_bootstrap_batch_failure_does_not_persist_partial_provisioning() {
        let mut world = fixture_world();
        let dir = std::env::temp_dir().join(format!(
            "oasis7-provider-bootstrap-atomic-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_dir_all(&dir);
        world
            .save_to_dir_with_chain_resource_context(
                &dir,
                ChainResourceDerivationContext {
                    world_id: "provision-world",
                    chain_id: "runtime-chain",
                    genesis_ref: None,
                    created_at_height: 0,
                    manifest_height: 0,
                    commit_block_hash: None,
                    tick: world.state().time,
                },
                "provider-bootstrap-config",
                "provider-bootstrap-generation",
            )
            .expect("save bootstrap baseline");

        let first = world
            .test_provider_backed_bootstrap_authority(
                "agent-a",
                "provider-bootstrap-atomic-a",
                "provider-bootstrap-authority",
                7,
            )
            .expect("build first provider bootstrap input");
        let mut second = first.clone();
        second.provision_id = "provider-bootstrap-atomic-b".to_string();
        second.owner_binding = "wrong-owner".to_string();

        let error = world
            .bootstrap_provider_backed_authorities(&[first, second])
            .expect_err("a later invalid authority must fail the whole batch");
        assert!(format!("{error:?}").contains("does not match live Runtime binding"));
        assert!(
            world
                .cognition_economy()
                .expect("read unchanged economy")
                .provision_journal
                .is_empty()
        );

        let restored = World::load_from_dir(&dir).expect("restore bootstrap baseline");
        assert!(
            restored
                .cognition_economy()
                .expect("read restored economy")
                .provision_journal
                .is_empty()
        );
        let _ = fs::remove_dir_all(dir);
    }
}

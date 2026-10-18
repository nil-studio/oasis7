use super::super::*;
use ed25519_dalek::Signer;
use oasis7_wasm_abi::{CapabilityAudience, CapabilityPresenter, CapabilitySubject};
use serde_json::json;
use std::collections::BTreeSet;

fn authority_transition_with_revocations(
    world: &World,
    revoked_grant_ids: BTreeSet<String>,
) -> (CapabilityAuthorityRecord, CapabilityAuthorityFinalityProof) {
    let mut record = world
        .capability_revocation_state()
        .authority_records
        .get(super::capability_grant_v2::ISSUER_ID)
        .cloned()
        .expect("fixture authority record");
    record.revocation_epoch = record.revocation_epoch.saturating_add(1);
    record.revoked_grant_ids = revoked_grant_ids;

    let mut proof = world
        .capability_revocation_state()
        .authority_finality_proofs
        .get(super::capability_grant_v2::ISSUER_ID)
        .cloned()
        .expect("fixture authority proof");
    proof.binding = CapabilityAuthorityFinalityBinding::from_record(&record)
        .expect("bind authority transition proof");
    proof.signatures.clear();
    for (node_id, signing_key) in [
        (
            super::capability_grant_v2::ISSUER_ID,
            super::capability_grant_v2::capability_issuer_signing_key(),
        ),
        (
            super::capability_grant_v2::FINALITY_SIGNER_2,
            super::capability_grant_v2::capability_finality_signing_key_2(),
        ),
    ] {
        let payload = proof
            .signing_payload_v1(node_id)
            .expect("encode authority transition proof payload");
        let signature = signing_key.sign(payload.as_slice());
        proof.signatures.insert(
            node_id.to_string(),
            format!(
                "{}{}",
                CapabilityAuthorityFinalityProof::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(signature.to_bytes())
            ),
        );
    }
    (record, proof)
}

#[test]
fn capability_authority_record_with_finality_proof_publication_failure_is_fully_unpublished() {
    let mut world = super::capability_grant_v2::fixture_world();
    let revoked_grant_id = world
        .capability_grants_v2()
        .keys()
        .next()
        .cloned()
        .expect("fixture has a grant to revoke");
    let (record, proof) =
        authority_transition_with_revocations(&world, BTreeSet::from([revoked_grant_id]));

    let snapshot_before = world.snapshot();
    let replay_manifest_hash = util::hash_json(&snapshot_before.manifest)
        .expect("hash capability authority replay manifest");
    let replay_snapshot_before = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: super::capability_grant_v2::WORLD_ID,
            chain_id: "runtime-chain",
            genesis_ref: None,
            created_at_height: snapshot_before.journal_len as u64,
            manifest_height: snapshot_before.journal_len as u64,
            commit_block_hash: None,
            tick: snapshot_before.state.time,
        },
        replay_manifest_hash.clone(),
        replay_manifest_hash,
    );
    assert_eq!(
        replay_snapshot_before.chain_resource_manifest.world_id,
        super::capability_grant_v2::WORLD_ID,
        "authority replay must use a snapshot bound to world.test"
    );
    let journal_before = world.journal().clone();
    let event_id_before = snapshot_before.last_event_id;
    let event_id_era_before = snapshot_before.event_id_era;
    let revocation_state_before = world.capability_revocation_state().clone();
    let authorization_root_before = world.capability_authorization_root().to_string();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let mut expected_world = world.clone();
    expected_world
        .install_capability_authority_record_with_finality_proof(record.clone(), proof.clone())
        .expect("proof/world checks pass for control authority transition");

    // A proof-bearing authority transition must use the staged authorization
    // publication seam. A post-prepare failure cannot expose a partial trust
    // root, revocation state, event id, journal entry, consensus record, or
    // deterministic backpressure update.
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .install_capability_authority_record_with_finality_proof(record.clone(), proof.clone())
        .expect_err("post-prepare failure must abort authority installation");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { ref reason }
            if reason.contains("publication preparation")
    ));

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.snapshot().last_event_id, event_id_before);
    assert_eq!(world.snapshot().event_id_era, event_id_era_before);
    assert_eq!(
        world.capability_revocation_state(),
        &revocation_state_before
    );
    assert_eq!(
        world.capability_authorization_root(),
        authorization_root_before
    );
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice()
    );
    assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);

    world
        .install_capability_authority_record_with_finality_proof(record.clone(), proof.clone())
        .expect("retry authority transition after one-shot failpoint");

    assert_eq!(world.snapshot(), expected_world.snapshot());
    assert_eq!(world.journal(), expected_world.journal());
    assert_eq!(
        world.capability_revocation_state(),
        expected_world.capability_revocation_state()
    );
    assert_eq!(
        world.capability_authorization_root(),
        expected_world.capability_authorization_root()
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected_world.tick_consensus_records()
    );
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        expected_world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected_world.runtime_backpressure_stats()
    );

    let tail = &world.journal().events[journal_before.events.len()..];
    assert_eq!(
        tail.len(),
        1,
        "authority transition must publish one authorization event"
    );
    assert!(matches!(
        &tail[0].body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::AuthorityInstalledWithProof {
                record: installed_record,
                proof: installed_proof,
            }
        ) if installed_record == &record && installed_proof == &proof
    ));

    let replayed = World::from_snapshot(replay_snapshot_before.clone(), world.journal().clone())
        .expect("replay successful authority transition");
    assert_eq!(replayed.state(), world.state());
    let mut expected_replay_snapshot = world.snapshot();
    expected_replay_snapshot.chain_resource_manifest =
        replay_snapshot_before.chain_resource_manifest;
    expected_replay_snapshot.latest_chain_resource_delta =
        replay_snapshot_before.latest_chain_resource_delta;
    assert_eq!(replayed.snapshot(), expected_replay_snapshot);
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.capability_revocation_state(),
        world.capability_revocation_state()
    );
    assert_eq!(
        replayed.capability_authorization_root(),
        world.capability_authorization_root()
    );
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_eq!(
        replayed.tick_consensus_rejection_audit_events(),
        world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        replayed.runtime_backpressure_stats(),
        world.runtime_backpressure_stats()
    );
}

#[test]
fn capability_agent_identity_publication_failure_is_fully_unpublished() {
    let mut world = super::capability_grant_v2::fixture_world();
    let agent_id = super::capability_grant_v2::SUBJECT_ID;
    let owner_binding = "owner-7";
    let generation = 2;

    let live_agent = world
        .state()
        .agents
        .get(agent_id)
        .expect("fixture has a live capability agent");
    assert_eq!(live_agent.state.agent_id, agent_id);
    let existing_identity = world
        .capability_revocation_state()
        .agent_identities
        .get(agent_id)
        .expect("fixture has the agent owner/generation binding");
    assert_eq!(existing_identity.owner_binding, owner_binding);
    assert_eq!(existing_identity.generation, 1);

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let event_id_before = snapshot_before.last_event_id;
    let event_id_era_before = snapshot_before.event_id_era;
    let agent_identities_before = world.capability_revocation_state().agent_identities.clone();
    let authorization_root_before = world.capability_authorization_root().to_string();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let mut expected_world = world.clone();

    // Agent identity publication must join the staged authorization batch. A
    // post-prepare failure must not expose the new owner/generation binding,
    // event id, journal entry, root, consensus record, or backpressure update.
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .install_capability_agent_identity(agent_id, owner_binding, generation)
        .expect_err("post-prepare failure must abort public agent identity installation");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { ref reason }
            if reason.contains("publication preparation")
    ));

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.snapshot().last_event_id, event_id_before);
    assert_eq!(world.snapshot().event_id_era, event_id_era_before);
    assert_eq!(
        world.capability_revocation_state().agent_identities,
        agent_identities_before
    );
    assert_eq!(
        world.capability_authorization_root(),
        authorization_root_before
    );
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice()
    );
    assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);

    expected_world
        .install_capability_agent_identity(agent_id, owner_binding, generation)
        .expect("control agent identity installation");
    world
        .install_capability_agent_identity(agent_id, owner_binding, generation)
        .expect("retry agent identity installation after one-shot failpoint");

    assert_eq!(world.snapshot(), expected_world.snapshot());
    assert_eq!(world.journal(), expected_world.journal());
    assert_eq!(
        world.capability_revocation_state().agent_identities,
        expected_world
            .capability_revocation_state()
            .agent_identities
    );
    assert_eq!(
        world.capability_authorization_root(),
        expected_world.capability_authorization_root()
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected_world.tick_consensus_records()
    );
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        expected_world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected_world.runtime_backpressure_stats()
    );

    let tail = &world.journal().events[journal_before.events.len()..];
    assert_eq!(
        tail.len(),
        1,
        "agent identity installation must publish one authorization event"
    );
    assert!(matches!(
        &tail[0].body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::AgentIdentityInstalled {
                agent_id: installed_agent_id,
                identity,
            }
        ) if installed_agent_id == agent_id
            && identity.owner_binding == owner_binding
            && identity.generation == generation
    ));

    let snapshot_after_install = world.snapshot();
    let journal_after_install = world.journal().clone();
    world
        .install_capability_agent_identity(agent_id, owner_binding, generation)
        .expect("same agent identity is idempotent");
    assert_eq!(world.snapshot(), snapshot_after_install);
    assert_eq!(world.journal(), &journal_after_install);

    let snapshot_before_regression = world.snapshot();
    let journal_before_regression = world.journal().clone();
    let error = world
        .install_capability_agent_identity(agent_id, owner_binding, 1)
        .expect_err("older agent identity generation must be rejected");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { ref reason }
            if reason.contains("generation regressed")
    ));
    assert_eq!(world.snapshot(), snapshot_before_regression);
    assert_eq!(world.journal(), &journal_before_regression);

    let replayed = World::from_snapshot(snapshot_before, world.journal().clone())
        .expect("replay successful agent identity installation");
    assert_eq!(replayed.state(), world.state());
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.snapshot().last_event_id,
        world.snapshot().last_event_id
    );
    assert_eq!(
        replayed.snapshot().event_id_era,
        world.snapshot().event_id_era
    );
    assert_eq!(
        replayed.capability_revocation_state().agent_identities,
        world.capability_revocation_state().agent_identities
    );
    assert_eq!(
        replayed.capability_authorization_root(),
        world.capability_authorization_root()
    );
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_eq!(
        replayed.tick_consensus_rejection_audit_events(),
        world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        replayed.runtime_backpressure_stats(),
        world.runtime_backpressure_stats()
    );
}

#[test]
fn capability_invocation_context_publication_failure_is_fully_unpublished() {
    let mut world = super::capability_grant_v2::fixture_world();
    let context = CapabilityInvocationContext {
        grant_id: "grant-context-publication-regression".to_string(),
        subject: CapabilitySubject::System {
            system_id: "system-context-publication-regression".to_string(),
            epoch: 0,
        },
        presenter: CapabilityPresenter {
            presenter_id: "provider-context-publication-regression".to_string(),
            presenter_kind: "provider".to_string(),
            session_id: Some("session-context-publication-regression".to_string()),
            attestation_ref: None,
        },
        audience: CapabilityAudience {
            world_id: "world.context-publication-regression".to_string(),
            branch_id: "branch-context-publication-regression".to_string(),
            finality_epoch: 0,
            target_kind: "world".to_string(),
            target_id: None,
        },
        catalog_snapshot_id: "catalog-context-publication-regression".to_string(),
        module_id: "module.context-publication-regression".to_string(),
        module_version: "1.0.0".to_string(),
        response_nonce: "response-context-publication-regression".to_string(),
    };

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let contexts_before = world.capability_invocation_contexts().clone();
    let system_identities_before = world
        .capability_revocation_state()
        .system_identities
        .clone();
    let authorization_root_before = world.capability_authorization_root().to_string();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let mut expected_world = world.clone();

    // The System subject must publish two authorization events. The failpoint
    // is after publication preparation, so neither event may install a
    // system identity, context, event id, journal entry, consensus record, or
    // backpressure update before the runtime transaction commits atomically.
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .install_capability_invocation_context(context.clone())
        .expect_err("post-prepare failure must abort public context installation");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { ref reason }
            if reason.contains("publication preparation")
    ));

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(
        world.snapshot().last_event_id,
        snapshot_before.last_event_id
    );
    assert_eq!(world.snapshot().event_id_era, snapshot_before.event_id_era);
    assert_eq!(world.capability_invocation_contexts(), &contexts_before);
    assert_eq!(
        world.capability_revocation_state().system_identities,
        system_identities_before
    );
    assert_eq!(
        world.capability_authorization_root(),
        authorization_root_before
    );
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice()
    );
    assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);

    expected_world
        .install_capability_invocation_context(context.clone())
        .expect("control context installation");
    world
        .install_capability_invocation_context(context)
        .expect("retry context installation after one-shot failpoint");

    assert_eq!(world.snapshot(), expected_world.snapshot());
    assert_eq!(world.journal(), expected_world.journal());
    assert_eq!(
        world.capability_invocation_contexts(),
        expected_world.capability_invocation_contexts()
    );
    assert_eq!(
        world.capability_revocation_state().system_identities,
        expected_world
            .capability_revocation_state()
            .system_identities
    );
    assert_eq!(
        world.capability_authorization_root(),
        expected_world.capability_authorization_root()
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected_world.tick_consensus_records()
    );
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        expected_world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected_world.runtime_backpressure_stats()
    );

    let tail = &world.journal().events[journal_before.events.len()..];
    assert_eq!(
        tail.len(),
        2,
        "System context installation must publish two events"
    );
    assert!(matches!(
        &tail[0].body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::SystemIdentityInstalled { system_id, epoch }
        ) if system_id == "system-context-publication-regression" && *epoch == 0
    ));
    assert!(matches!(
        &tail[1].body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::InvocationContextInstalled { context: installed, .. }
        ) if installed.grant_id == "grant-context-publication-regression"
            && installed.response_nonce == "response-context-publication-regression"
            && matches!(
                &installed.subject,
                CapabilitySubject::System { system_id, epoch }
                    if system_id == "system-context-publication-regression" && *epoch == 0
            )
    ));

    let replayed = World::from_snapshot(snapshot_before, world.journal().clone())
        .expect("replay successful context installation");
    assert_eq!(replayed.state(), world.state());
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.snapshot().last_event_id,
        world.snapshot().last_event_id
    );
    assert_eq!(
        replayed.snapshot().event_id_era,
        world.snapshot().event_id_era
    );
    assert_eq!(
        replayed.capability_invocation_contexts(),
        world.capability_invocation_contexts()
    );
    assert_eq!(
        replayed.capability_revocation_state().system_identities,
        world.capability_revocation_state().system_identities
    );
    assert_eq!(
        replayed.capability_authorization_root(),
        world.capability_authorization_root()
    );
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_eq!(
        replayed.tick_consensus_rejection_audit_events(),
        world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        replayed.runtime_backpressure_stats(),
        world.runtime_backpressure_stats()
    );
}

#[test]
fn capability_budget_account_publication_failure_is_fully_unpublished() {
    let mut world = super::capability_grant_v2::fixture_world();
    let subject = world
        .capability_budget_accounts()
        .values()
        .next()
        .expect("fixture has a valid budget subject")
        .subject
        .clone();
    let account = CapabilityBudgetAccount {
        subject,
        grant_id: "grant-budget-account-publication-regression".to_string(),
        remaining_units: 64,
        reserved_units: 0,
        spent_units: 0,
    };

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let event_id_before = snapshot_before.last_event_id;
    let event_id_era_before = snapshot_before.event_id_era;
    let budget_accounts_before = world.capability_budget_accounts().clone();
    let authorization_root_before = world.capability_authorization_root().to_string();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let mut expected_world = world.clone();

    // The public budget installer still routes through the legacy append path.
    // The injected post-prepare failure must therefore abort before the budget
    // account, event id, journal, authorization root, consensus, or
    // deterministic backpressure can become observable.
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .install_capability_budget_account(account.clone())
        .expect_err("post-prepare failure must abort public budget installation");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { ref reason }
            if reason.contains("publication preparation")
    ));

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.snapshot().last_event_id, event_id_before);
    assert_eq!(world.snapshot().event_id_era, event_id_era_before);
    assert_eq!(world.capability_budget_accounts(), &budget_accounts_before);
    assert_eq!(
        world.capability_authorization_root(),
        authorization_root_before
    );
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice()
    );
    assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);

    expected_world
        .install_capability_budget_account(account.clone())
        .expect("control budget account installation");
    world
        .install_capability_budget_account(account.clone())
        .expect("retry budget account installation after one-shot failpoint");

    assert_eq!(world.snapshot(), expected_world.snapshot());
    assert_eq!(world.journal(), expected_world.journal());
    assert_eq!(
        world.snapshot().last_event_id,
        expected_world.snapshot().last_event_id
    );
    assert_eq!(
        world.snapshot().event_id_era,
        expected_world.snapshot().event_id_era
    );
    assert_eq!(
        world.capability_budget_accounts(),
        expected_world.capability_budget_accounts()
    );
    assert_eq!(
        world.capability_authorization_root(),
        expected_world.capability_authorization_root()
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected_world.tick_consensus_records()
    );
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        expected_world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected_world.runtime_backpressure_stats()
    );

    let tail = &world.journal().events[journal_before.events.len()..];
    assert_eq!(
        tail.len(),
        1,
        "budget account installation must publish one event"
    );
    assert!(matches!(
        &tail[0].body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::BudgetAccountInstalled {
                account: installed,
                ..
            }
        ) if installed == &account
    ));

    let replayed = World::from_snapshot(snapshot_before, world.journal().clone())
        .expect("replay successful budget account installation");
    assert_eq!(replayed.state(), world.state());
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.snapshot().last_event_id,
        world.snapshot().last_event_id
    );
    assert_eq!(
        replayed.snapshot().event_id_era,
        world.snapshot().event_id_era
    );
    assert_eq!(
        replayed.capability_budget_accounts(),
        world.capability_budget_accounts()
    );
    assert_eq!(
        replayed.capability_authorization_root(),
        world.capability_authorization_root()
    );
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_eq!(
        replayed.tick_consensus_rejection_audit_events(),
        world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        replayed.runtime_backpressure_stats(),
        world.runtime_backpressure_stats()
    );
}

#[test]
fn capability_grant_registration_publication_failure_is_fully_unpublished() {
    let mut world = super::capability_grant_v2::fixture_world();
    let grant =
        super::capability_grant_v2::signed_grant(super::capability_grant_v2::grant_json(json!({
            "grant_nonce": "grant-registration-publication-regression"
        })));

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let event_id_before = snapshot_before.last_event_id;
    let event_id_era_before = snapshot_before.event_id_era;
    let grants_before = world.capability_grants_v2().clone();
    let authorization_root_before = world.capability_authorization_root().to_string();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let mut expected_world = world.clone();

    // The public grant-registration API still routes through the legacy
    // append path. A post-prepare failure must leave every durable grant and
    // publication projection untouched until the typed transaction commits.
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .register_capability_grant_v2(grant.clone())
        .expect_err("post-prepare failure must abort public grant registration");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { ref reason }
            if reason.contains("publication preparation")
    ));

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.snapshot().last_event_id, event_id_before);
    assert_eq!(world.snapshot().event_id_era, event_id_era_before);
    assert_eq!(world.capability_grants_v2(), &grants_before);
    assert_eq!(
        world.capability_authorization_root(),
        authorization_root_before
    );
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice()
    );
    assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);

    expected_world
        .register_capability_grant_v2(grant.clone())
        .expect("control grant registration");
    world
        .register_capability_grant_v2(grant.clone())
        .expect("retry grant registration after one-shot failpoint");

    assert_eq!(world.snapshot(), expected_world.snapshot());
    assert_eq!(world.journal(), expected_world.journal());
    assert_eq!(
        world.snapshot().last_event_id,
        expected_world.snapshot().last_event_id
    );
    assert_eq!(
        world.snapshot().event_id_era,
        expected_world.snapshot().event_id_era
    );
    assert_eq!(
        world.capability_grants_v2(),
        expected_world.capability_grants_v2()
    );
    assert_eq!(
        world.capability_authorization_root(),
        expected_world.capability_authorization_root()
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected_world.tick_consensus_records()
    );
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        expected_world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected_world.runtime_backpressure_stats()
    );

    let tail = &world.journal().events[journal_before.events.len()..];
    assert_eq!(
        tail.len(),
        1,
        "grant registration must publish one authorization event"
    );
    assert!(matches!(
        &tail[0].body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::GrantRegistered { grant: installed }
        ) if installed == &grant
    ));

    let replayed = World::from_snapshot(snapshot_before, world.journal().clone())
        .expect("replay successful grant registration");
    assert_eq!(replayed.state(), world.state());
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.snapshot().last_event_id,
        world.snapshot().last_event_id
    );
    assert_eq!(
        replayed.snapshot().event_id_era,
        world.snapshot().event_id_era
    );
    assert_eq!(
        replayed.capability_grants_v2(),
        world.capability_grants_v2()
    );
    assert_eq!(
        replayed.capability_authorization_root(),
        world.capability_authorization_root()
    );
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_eq!(
        replayed.tick_consensus_rejection_audit_events(),
        world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        replayed.runtime_backpressure_stats(),
        world.runtime_backpressure_stats()
    );
}

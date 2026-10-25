//! Deterministic migration, replay, management, and lease reducers.
use super::super::{
    FormalError, FullSyncIssuerAnchorV2, FullSyncIssuerCommittedEntryV2, FullSyncIssuerTargetV2,
    SignedFullSyncIssuerObservationV2, VerifiedFullSyncIssuerPolicyV2,
    VerifiedFullSyncIssuerStateV2, canonical, digest, encode, invalid,
};
use super::state::{
    FullSyncIssuerCommittedMigrationResumeV3, FullSyncIssuerMigrationResumeResultV3,
    FullSyncIssuerRequestLocationV3, PreparedFullSyncIssuerEventV3,
    VerifiedFullSyncIssuerDecisionV3, VerifiedFullSyncIssuerStateV3,
};
use super::types::FullSyncIssuerReceiptObservationV3 as ReceiptObservationV3;
use super::types::{
    FullSyncIssuerAnchorV3, FullSyncIssuerCommittedEntryV3, FullSyncIssuerEventV3,
    FullSyncIssuerOperationV3, FullSyncIssuerPreviousAnchorV3, FullSyncIssuerRecordV3,
    FullSyncIssuerTargetLeaseV3, FullSyncIssuerTargetStateV3 as TargetStateV3,
    FullSyncProviderLeaseBodyV3, FullSyncProviderLeaseStateV3, LOG_LINK_DOMAIN_V3,
    MAX_ENTRY_BYTES_V3, MAX_FULL_SYNC_ISSUER_LEDGER_RECORDS_V3,
    MAX_FULL_SYNC_PROVIDER_LEASE_MILLIS_V3, MAX_GRANT_MILLIS_V3, MAX_LEDGER_BYTES_V3,
    MAX_STORED_ENTRY_BYTES_V3, STATE_DOMAIN_V3, STORED_ENTRY_DOMAIN_V3,
    SignedFullSyncIssuerCommandV3, SignedFullSyncIssuerMigrationV3, SignedFullSyncIssuerReceiptV3,
    SignedFullSyncProviderLeaseRequestV3, SignedFullSyncProviderLeaseV3,
    VerifiedFullSyncIssuerPolicyV3,
};
use super::witness::{
    FullSyncIssuerHistoryV3, FullSyncIssuerMigrationResumeBundleV3,
    FullSyncIssuerWitnessModeHeadV3, SignedFullSyncIssuerMigrationResumeQueryV3,
    SignedFullSyncIssuerObservationV3,
};
use ed25519_dalek::SigningKey;
use std::collections::{BTreeMap, BTreeSet};

const EVENT_DIGEST_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.event.v3";
const REQUEST_DIGEST_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.request-index.v3";
const REGISTRY_DIGEST_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.registry.v3";
const RECEIPT_RESERVE_BYTES_V3: usize = 64 * 1024;

struct EventStateChangesV3 {
    target_states: BTreeMap<FullSyncIssuerTargetV2, TargetStateV3>,
    leases: BTreeMap<FullSyncIssuerTargetLeaseV3, FullSyncProviderLeaseStateV3>,
    manager_nonce: Option<(String, String)>,
    provider_nonce: Option<(String, String)>,
    lease_ids: Option<BTreeSet<(String, String, String)>>,
}

struct AnchorStateV3<'a> {
    v2_head: &'a FullSyncIssuerAnchorV2,
    policy: &'a VerifiedFullSyncIssuerPolicyV3,
    target_states: &'a BTreeMap<FullSyncIssuerTargetV2, TargetStateV3>,
    leases: &'a BTreeMap<FullSyncIssuerTargetLeaseV3, FullSyncProviderLeaseStateV3>,
    request_digests: &'a BTreeMap<String, String>,
    manager_nonces: &'a BTreeSet<(String, String)>,
    provider_nonces: &'a BTreeSet<(String, String)>,
    lease_ids: &'a BTreeSet<(String, String, String)>,
    witness_nonces: &'a BTreeSet<(String, String, String)>,
}

pub(super) fn prepare_migration(
    state: &VerifiedFullSyncIssuerStateV2,
    new_policy: VerifiedFullSyncIssuerPolicyV3,
    migration: SignedFullSyncIssuerMigrationV3,
    observation: &SignedFullSyncIssuerObservationV2,
) -> Result<PreparedFullSyncIssuerEventV3, FormalError> {
    let request_id = migration.body.request_id.clone();
    canonical::<32>(&request_id)?;
    if state.requests.contains_key(&request_id) {
        return Err(FormalError::Conflict(
            "full-sync V3 migration request identity already used".into(),
        ));
    }
    // The externally root-verified configured V2 policy is the effective
    // manager policy only at the untouched genesis anchor. After any V2
    // commit, migration must use the policy authenticated by that history.
    let old_policy = match state.active_policy.as_ref() {
        Some(policy) => policy,
        None if state.anchor.revision == 0 => &state.configured_policy,
        None => {
            return Err(invalid(
                "full-sync V3 migration requires an active V2 policy",
            ));
        }
    };
    let verified_new_policy = VerifiedFullSyncIssuerPolicyV3::verify(
        new_policy.signed(),
        state.configured_policy.trust(),
        old_policy,
    )?;
    if verified_new_policy.policy_digest() != new_policy.policy_digest() {
        return Err(invalid(
            "full-sync V3 migration policy verification changed",
        ));
    }
    observation.verify(old_policy)?;
    if observation.body.current_anchor != state.anchor
        || observation.body.query.body.minimum_anchor != state.anchor
    {
        return Err(FormalError::Conflict(
            "full-sync V3 migration requires an exact current V2 observation".into(),
        ));
    }
    migration.verify(old_policy, &verified_new_policy)?;
    if migration.body.expected_v2_head != state.anchor {
        return Err(FormalError::Conflict(
            "full-sync V3 migration expected V2 head compare-and-set".into(),
        ));
    }
    let manager_nonce = (
        old_policy.body().management_public_key.clone(),
        migration.body.manager_nonce.clone(),
    );
    if state.manager_nonces.contains(&manager_nonce) {
        return Err(FormalError::Conflict(
            "full-sync V3 migration manager nonce already used".into(),
        ));
    }
    if state.entries.len() >= MAX_FULL_SYNC_ISSUER_LEDGER_RECORDS_V3 {
        return Err(invalid("full-sync V3 total ledger record capacity"));
    }
    let mut witness_nonces = collect_v2_witness_nonces(&state.entries)?;
    let migration_nonce = witness_nonce_v2(observation);
    if !witness_nonces.insert(migration_nonce) {
        return Err(FormalError::Conflict(
            "full-sync V3 migration witness nonce already used".into(),
        ));
    }

    let event = FullSyncIssuerEventV3::Migration(Box::new(migration.clone()));
    let target_states = state
        .target_states
        .iter()
        .map(|(target, value)| {
            (
                target.clone(),
                TargetStateV3 {
                    target: value.target.clone(),
                    generation: value.generation,
                    active: value.active,
                    not_before_unix_ms: value.not_before_unix_ms,
                    expires_unix_ms: value.expires_unix_ms,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let leases = BTreeMap::new();
    let mut request_ids = state
        .requests
        .iter()
        .map(|(id, index)| (id.clone(), FullSyncIssuerRequestLocationV3::V2(*index)))
        .collect::<BTreeMap<_, _>>();
    let mut request_digests = state.request_digests.clone();
    let mut manager_nonces = state.manager_nonces.clone();
    request_ids.insert(request_id.clone(), FullSyncIssuerRequestLocationV3::V3(0));
    request_digests.insert(request_id, event_request_digest(&event)?);
    manager_nonces.insert(manager_nonce);

    let previous_anchor = FullSyncIssuerPreviousAnchorV3::V2(state.anchor.clone());
    let provider_nonces = BTreeSet::new();
    let lease_ids = BTreeSet::new();
    let next_anchor = successor_anchor_v3(
        &previous_anchor,
        &event,
        AnchorStateV3 {
            v2_head: &state.anchor,
            policy: &verified_new_policy,
            target_states: &target_states,
            leases: &leases,
            request_digests: &request_digests,
            manager_nonces: &manager_nonces,
            provider_nonces: &provider_nonces,
            lease_ids: &lease_ids,
            witness_nonces: &witness_nonces,
        },
    )?;
    let record = FullSyncIssuerRecordV3 {
        schema_version: 3,
        policy: verified_new_policy.signed().clone(),
        previous_anchor,
        event,
        next_anchor: next_anchor.clone(),
    };
    ensure_capacity_v3(state.encoded_bytes, state.entries.len(), &record)?;
    let next_state = VerifiedFullSyncIssuerStateV3 {
        v2_policy: old_policy.clone(),
        policy: verified_new_policy,
        v2_entries: state.entries.clone(),
        entries: Vec::new(),
        anchor: next_anchor,
        v2_head: state.anchor.clone(),
        target_states,
        leases,
        request_ids,
        request_digests,
        manager_nonces,
        provider_nonces: BTreeSet::new(),
        lease_ids: BTreeSet::new(),
        witness_nonces,
        encoded_bytes: state.encoded_bytes,
    };
    Ok(PreparedFullSyncIssuerEventV3 {
        record: Some(record),
        existing: None,
        next_state: Some(next_state),
        old_policy: old_policy.clone(),
        expected_observation: ReceiptObservationV3::Migration(observation.clone()),
    })
}

pub(super) fn reconstruct(
    v2_policy: VerifiedFullSyncIssuerPolicyV2,
    v3_policy: VerifiedFullSyncIssuerPolicyV3,
    history: FullSyncIssuerHistoryV3,
    observation: &SignedFullSyncIssuerObservationV3,
) -> Result<VerifiedFullSyncIssuerStateV3, FormalError> {
    observation.verify(&v3_policy)?;
    let state = reconstruct_history(v2_policy, v3_policy, history)?;
    let minimum = &observation.body.query.body.minimum_anchor;
    let prefix = anchor_at_v3_revision(&state, minimum.revision)?;
    if prefix != *minimum || state.anchor != observation.body.current_anchor {
        return Err(FormalError::Rollback(
            "full-sync V3 records do not close observed head and minimum".into(),
        ));
    }
    Ok(state)
}

fn reconstruct_history(
    v2_policy: VerifiedFullSyncIssuerPolicyV2,
    v3_policy: VerifiedFullSyncIssuerPolicyV3,
    history: FullSyncIssuerHistoryV3,
) -> Result<VerifiedFullSyncIssuerStateV3, FormalError> {
    let total = history
        .v2_entries
        .len()
        .checked_add(history.v3_entries.len())
        .ok_or_else(|| invalid("full-sync V3 record count overflow"))?;
    if history.v3_entries.is_empty() || total > MAX_FULL_SYNC_ISSUER_LEDGER_RECORDS_V3 {
        return Err(invalid("full-sync V3 history migration/count bound"));
    }
    let v2_state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(v2_policy, history.v2_entries)?;
    let migration_entry = history
        .v3_entries
        .first()
        .ok_or_else(|| invalid("full-sync V3 history lacks migration"))?;
    let (migration, observation) = match (
        &migration_entry.record.event,
        &migration_entry.receipt.body.observation,
    ) {
        (
            FullSyncIssuerEventV3::Migration(migration),
            ReceiptObservationV3::Migration(observation),
        ) => (migration.as_ref().clone(), observation.clone()),
        _ => {
            return Err(invalid(
                "full-sync V3 history does not start with migration",
            ));
        }
    };
    let prepared = v2_state.prepare_migration_v3(v3_policy, migration, &observation)?;
    if prepared.record() != Some(&migration_entry.record) {
        return Err(invalid(
            "full-sync V3 migration record differs from reducer",
        ));
    }
    let mut state = prepared
        .commit(migration_entry.receipt.clone())?
        .state()
        .clone();
    for entry in history.v3_entries.into_iter().skip(1) {
        let observation = match &entry.receipt.body.observation {
            ReceiptObservationV3::V3(observation) => observation.clone(),
            ReceiptObservationV3::Migration(_) => {
                return Err(invalid("full-sync V3 post-migration receipt schema"));
            }
        };
        let prepared = match &entry.record.event {
            FullSyncIssuerEventV3::Migration(_) => {
                return Err(invalid("full-sync V3 migration event is not unique"));
            }
            FullSyncIssuerEventV3::Management(command) => {
                prepare_management(&state, command.as_ref().clone(), &observation)?
            }
            FullSyncIssuerEventV3::Lease { request, lease } => prepare_lease_record(
                &state,
                request.as_ref().clone(),
                lease.as_ref().clone(),
                &observation,
            )?,
        };
        if prepared.record() != Some(&entry.record) {
            return Err(invalid("full-sync V3 committed entry differs from reducer"));
        }
        state = prepared.commit(entry.receipt)?.state().clone();
    }
    Ok(state)
}

pub(super) fn prepare_management(
    state: &VerifiedFullSyncIssuerStateV3,
    command: SignedFullSyncIssuerCommandV3,
    observation: &SignedFullSyncIssuerObservationV3,
) -> Result<PreparedFullSyncIssuerEventV3, FormalError> {
    let request_id = command.body.request_id.clone();
    canonical::<32>(&request_id)?;
    if let Some(location) = state.request_ids.get(&request_id) {
        return match location {
            FullSyncIssuerRequestLocationV3::V2(_) => Err(FormalError::Conflict(
                "full-sync V3 request identity reused from V2 prefix".into(),
            )),
            FullSyncIssuerRequestLocationV3::V3(index) => {
                let original = state
                    .entries
                    .get(*index)
                    .ok_or_else(|| invalid("full-sync V3 request index"))?;
                match &original.record.event {
                    FullSyncIssuerEventV3::Management(existing)
                        if existing.as_ref() == &command =>
                    {
                        Ok(prepared_existing(state, original))
                    }
                    _ => Err(FormalError::Conflict(
                        "full-sync V3 request identity content conflict".into(),
                    )),
                }
            }
        };
    }
    command.verify(&state.policy)?;
    verify_observation_at_head(state, observation)?;
    if command.body.expected_head != state.anchor {
        return Err(FormalError::Conflict(
            "full-sync V3 management head compare-and-set".into(),
        ));
    }
    let nonce = (
        state.policy.body().management_public_key.clone(),
        command.body.command_nonce.clone(),
    );
    if state.manager_nonces.contains(&nonce) {
        return Err(FormalError::Conflict(
            "full-sync V3 management nonce already used".into(),
        ));
    }
    let current_known = state.target_states.contains_key(&command.body.target);
    let role_allowed = state
        .policy
        .body()
        .allowed_roles
        .contains(&command.body.target.role);
    let revokes_known_target =
        current_known && matches!(&command.body.operation, FullSyncIssuerOperationV3::Revoke);
    if !(role_allowed || revokes_known_target) {
        return Err(invalid("full-sync V3 management role not allowed"));
    }
    let resulting = transition_target_v3(&state.target_states, &command)?;
    if !current_known && state.target_states.len() >= super::super::MAX_TARGETS {
        return Err(invalid("full-sync V3 target capacity"));
    }
    let mut target_states = state.target_states.clone();
    target_states.insert(resulting.target.clone(), resulting);
    prepare_new_event(
        state,
        FullSyncIssuerEventV3::Management(Box::new(command)),
        ReceiptObservationV3::V3(observation.clone()),
        EventStateChangesV3 {
            target_states,
            leases: state.leases.clone(),
            manager_nonce: Some(nonce),
            provider_nonce: None,
            lease_ids: None,
        },
    )
}

pub(super) fn prepare_lease(
    state: &VerifiedFullSyncIssuerStateV3,
    request: SignedFullSyncProviderLeaseRequestV3,
    observation: &SignedFullSyncIssuerObservationV3,
    lease_signer: &SigningKey,
) -> Result<PreparedFullSyncIssuerEventV3, FormalError> {
    let request_id = request.body.request_id.clone();
    canonical::<32>(&request_id)?;
    if let Some(location) = state.request_ids.get(&request_id) {
        return match location {
            FullSyncIssuerRequestLocationV3::V2(_) => Err(FormalError::Conflict(
                "full-sync V3 request identity reused from V2 prefix".into(),
            )),
            FullSyncIssuerRequestLocationV3::V3(index) => {
                let original = state
                    .entries
                    .get(*index)
                    .ok_or_else(|| invalid("full-sync V3 request index"))?;
                match &original.record.event {
                    FullSyncIssuerEventV3::Lease {
                        request: existing, ..
                    } if existing.as_ref() == &request => Ok(prepared_existing(state, original)),
                    _ => Err(FormalError::Conflict(
                        "full-sync V3 request identity content conflict".into(),
                    )),
                }
            }
        };
    }
    let lease = derive_and_sign_lease(state, &request, observation, lease_signer)?;
    prepare_lease_record(state, request, lease, observation)
}

fn prepare_lease_record(
    state: &VerifiedFullSyncIssuerStateV3,
    request: SignedFullSyncProviderLeaseRequestV3,
    lease: SignedFullSyncProviderLeaseV3,
    observation: &SignedFullSyncIssuerObservationV3,
) -> Result<PreparedFullSyncIssuerEventV3, FormalError> {
    let request_id = request.body.request_id.clone();
    canonical::<32>(&request_id)?;
    if let Some(location) = state.request_ids.get(&request_id) {
        return match location {
            FullSyncIssuerRequestLocationV3::V2(_) => Err(FormalError::Conflict(
                "full-sync V3 request identity reused from V2 prefix".into(),
            )),
            FullSyncIssuerRequestLocationV3::V3(index) => {
                let original = state
                    .entries
                    .get(*index)
                    .ok_or_else(|| invalid("full-sync V3 request index"))?;
                match &original.record.event {
                    FullSyncIssuerEventV3::Lease {
                        request: existing, ..
                    } if existing.as_ref() == &request => Ok(prepared_existing(state, original)),
                    _ => Err(FormalError::Conflict(
                        "full-sync V3 request identity content conflict".into(),
                    )),
                }
            }
        };
    }
    request.verify(&state.policy)?;
    verify_observation_at_head(state, observation)?;
    if request.body.expected_head != state.anchor {
        return Err(FormalError::Conflict(
            "full-sync V3 provider lease head compare-and-set".into(),
        ));
    }
    lease.verify_issuer_signature(&state.policy)?;
    let expected = derive_lease_body(state, &request, observation)?;
    if lease.body != expected {
        return Err(invalid("full-sync V3 provider lease differs from reducer"));
    }
    let provider_nonce = (
        request.body.provider_public_key.clone(),
        request.body.request_nonce.clone(),
    );
    if state.provider_nonces.contains(&provider_nonce) {
        return Err(FormalError::Conflict(
            "full-sync V3 provider nonce already used".into(),
        ));
    }
    let lease_key = FullSyncIssuerTargetLeaseV3 {
        provider_public_key: request.body.provider_public_key.clone(),
        provider_incarnation: request.body.provider_incarnation.clone(),
        lease_id: request.body.lease_id.clone(),
    };
    let lease_identity = (
        lease_key.provider_public_key.clone(),
        lease_key.provider_incarnation.clone(),
        lease_key.lease_id.clone(),
    );
    if state.lease_ids.contains(&lease_identity) {
        return Err(FormalError::Conflict(
            "full-sync V3 provider lease identity already used".into(),
        ));
    }
    let mut leases = state.leases.clone();
    leases.insert(
        lease_key,
        FullSyncProviderLeaseStateV3 {
            request: request.clone(),
            lease: lease.clone(),
        },
    );
    let mut provider_nonces = state.provider_nonces.clone();
    provider_nonces.insert(provider_nonce.clone());
    let mut lease_ids = state.lease_ids.clone();
    lease_ids.insert(lease_identity);
    prepare_new_event(
        state,
        FullSyncIssuerEventV3::Lease {
            request: Box::new(request),
            lease: Box::new(lease),
        },
        ReceiptObservationV3::V3(observation.clone()),
        EventStateChangesV3 {
            target_states: state.target_states.clone(),
            leases,
            manager_nonce: None,
            provider_nonce: Some(provider_nonce),
            lease_ids: Some(lease_ids),
        },
    )
    .map(|mut prepared| {
        if let Some(next) = prepared.next_state.as_mut() {
            next.provider_nonces = provider_nonces;
        }
        prepared
    })
}

fn prepared_existing(
    state: &VerifiedFullSyncIssuerStateV3,
    entry: &FullSyncIssuerCommittedEntryV3,
) -> PreparedFullSyncIssuerEventV3 {
    PreparedFullSyncIssuerEventV3 {
        record: None,
        existing: Some(entry.clone()),
        next_state: None,
        old_policy: state.v2_policy.clone(),
        expected_observation: entry.receipt.body.observation.clone(),
    }
}

fn prepare_new_event(
    state: &VerifiedFullSyncIssuerStateV3,
    event: FullSyncIssuerEventV3,
    expected_observation: ReceiptObservationV3,
    changes: EventStateChangesV3,
) -> Result<PreparedFullSyncIssuerEventV3, FormalError> {
    let EventStateChangesV3 {
        target_states,
        leases,
        manager_nonce,
        provider_nonce,
        lease_ids,
    } = changes;
    ensure_next_event_capacity(state)?;
    if target_states.len() > super::super::MAX_TARGETS {
        return Err(invalid("full-sync V3 target capacity"));
    }
    let request_id = event_request_id(&event).to_owned();
    if state.request_ids.contains_key(&request_id) {
        return Err(FormalError::Conflict(
            "full-sync V3 event request identity already used".into(),
        ));
    }
    let mut next_state = state.clone();
    next_state.target_states = target_states;
    next_state.leases = leases;
    if let Some(nonce) = manager_nonce
        && !next_state.manager_nonces.insert(nonce)
    {
        return Err(FormalError::Conflict(
            "full-sync V3 management nonce already used".into(),
        ));
    }
    if let Some(nonce) = provider_nonce
        && !next_state.provider_nonces.insert(nonce)
    {
        return Err(FormalError::Conflict(
            "full-sync V3 provider nonce already used".into(),
        ));
    }
    if let Some(ids) = lease_ids {
        next_state.lease_ids = ids;
    }
    let witness_nonce = witness_nonce_v3(&expected_observation);
    if !next_state.witness_nonces.insert(witness_nonce) {
        return Err(FormalError::Conflict(
            "full-sync V3 witness nonce already used".into(),
        ));
    }
    next_state.request_ids.insert(
        request_id.clone(),
        FullSyncIssuerRequestLocationV3::V3(state.entries.len()),
    );
    next_state
        .request_digests
        .insert(request_id, event_request_digest(&event)?);
    let previous_anchor = FullSyncIssuerPreviousAnchorV3::V3(state.anchor.clone());
    let next_anchor = successor_anchor_v3(
        &previous_anchor,
        &event,
        AnchorStateV3 {
            v2_head: &state.v2_head,
            policy: &state.policy,
            target_states: &next_state.target_states,
            leases: &next_state.leases,
            request_digests: &next_state.request_digests,
            manager_nonces: &next_state.manager_nonces,
            provider_nonces: &next_state.provider_nonces,
            lease_ids: &next_state.lease_ids,
            witness_nonces: &next_state.witness_nonces,
        },
    )?;
    let record = FullSyncIssuerRecordV3 {
        schema_version: 3,
        policy: state.policy.signed().clone(),
        previous_anchor,
        event,
        next_anchor: next_anchor.clone(),
    };
    ensure_capacity_v3(
        state.encoded_bytes,
        state.v2_entries.len() + state.entries.len(),
        &record,
    )?;
    next_state.anchor = next_anchor;
    Ok(PreparedFullSyncIssuerEventV3 {
        record: Some(record),
        existing: None,
        next_state: Some(next_state),
        old_policy: state.v2_policy.clone(),
        expected_observation,
    })
}

fn transition_target_v3(
    target_states: &BTreeMap<FullSyncIssuerTargetV2, TargetStateV3>,
    command: &SignedFullSyncIssuerCommandV3,
) -> Result<TargetStateV3, FormalError> {
    let body = &command.body;
    let current = target_states.get(&body.target);
    let current_generation = current.map_or(0, |state| state.generation);
    if body.expected_target_generation != current_generation {
        return Err(FormalError::Conflict(
            "full-sync V3 target generation compare-and-set".into(),
        ));
    }
    let generation = current_generation
        .checked_add(1)
        .ok_or_else(|| invalid("full-sync V3 target generation overflow"))?;
    let (active, not_before_unix_ms, expires_unix_ms) = match &body.operation {
        FullSyncIssuerOperationV3::Issue {
            not_before_unix_ms,
            expires_unix_ms,
        } => {
            if current.is_some_and(|state| state.active) {
                return Err(FormalError::Conflict(
                    "full-sync V3 issue requires inactive target".into(),
                ));
            }
            (true, Some(*not_before_unix_ms), Some(*expires_unix_ms))
        }
        FullSyncIssuerOperationV3::Replace {
            not_before_unix_ms,
            expires_unix_ms,
        } => {
            if !current.is_some_and(|state| state.active) {
                return Err(FormalError::Conflict(
                    "full-sync V3 replace requires active target".into(),
                ));
            }
            (true, Some(*not_before_unix_ms), Some(*expires_unix_ms))
        }
        FullSyncIssuerOperationV3::Revoke => (false, None, None),
    };
    let resulting = TargetStateV3 {
        target: body.target.clone(),
        generation,
        active,
        not_before_unix_ms,
        expires_unix_ms,
    };
    validate_target_state_v3(&resulting)?;
    Ok(resulting)
}

fn validate_target_state_v3(state: &TargetStateV3) -> Result<(), FormalError> {
    state.target.validate()?;
    if state.generation == 0 {
        return Err(invalid("full-sync V3 target state generation"));
    }
    match (
        state.active,
        state.not_before_unix_ms,
        state.expires_unix_ms,
    ) {
        (true, Some(not_before), Some(expires)) => {
            let duration = expires
                .checked_sub(not_before)
                .ok_or_else(|| invalid("full-sync V3 target grant expiry"))?;
            if duration == 0 || duration > MAX_GRANT_MILLIS_V3 {
                return Err(invalid("full-sync V3 target grant duration"));
            }
        }
        (false, None, None) => {}
        _ => return Err(invalid("full-sync V3 target active grant fields")),
    }
    Ok(())
}

fn verify_observation_at_head(
    state: &VerifiedFullSyncIssuerStateV3,
    observation: &SignedFullSyncIssuerObservationV3,
) -> Result<(), FormalError> {
    observation.verify(&state.policy)?;
    if observation.body.current_anchor != state.anchor
        || observation.body.query.body.minimum_anchor != state.anchor
    {
        return Err(FormalError::Conflict(
            "full-sync V3 exact current head observation required".into(),
        ));
    }
    Ok(())
}

fn derive_and_sign_lease(
    state: &VerifiedFullSyncIssuerStateV3,
    request: &SignedFullSyncProviderLeaseRequestV3,
    observation: &SignedFullSyncIssuerObservationV3,
    lease_signer: &SigningKey,
) -> Result<SignedFullSyncProviderLeaseV3, FormalError> {
    request.verify(&state.policy)?;
    verify_observation_at_head(state, observation)?;
    if request.body.expected_head != state.anchor {
        return Err(FormalError::Conflict(
            "full-sync V3 provider lease head compare-and-set".into(),
        ));
    }
    let body = derive_lease_body(state, request, observation)?;
    SignedFullSyncProviderLeaseV3::sign(body, &state.policy, lease_signer)
}

fn derive_lease_body(
    state: &VerifiedFullSyncIssuerStateV3,
    request: &SignedFullSyncProviderLeaseRequestV3,
    observation: &SignedFullSyncIssuerObservationV3,
) -> Result<FullSyncProviderLeaseBodyV3, FormalError> {
    observation.verify(&state.policy)?;
    let body = &request.body;
    let target = state
        .target_states
        .get(&body.target)
        .ok_or_else(|| invalid("full-sync V3 provider target is not issued"))?;
    if !target.active
        || target.generation != body.target_generation
        || !state
            .policy
            .body()
            .allowed_roles
            .contains(&body.target.role)
    {
        return Err(FormalError::Conflict(
            "full-sync V3 provider target is inactive or stale".into(),
        ));
    }
    let (grant_start, grant_end) = match (target.not_before_unix_ms, target.expires_unix_ms) {
        (Some(start), Some(end)) => (start, end),
        _ => return Err(invalid("full-sync V3 active target has no grant interval")),
    };
    let time = &observation.body.protected_time;
    let not_before_unix_ms = time.latest_unix_ms.max(grant_start);
    let conservative_end = time
        .earliest_unix_ms
        .checked_add(state.policy.body().max_authorization_staleness_ms)
        .ok_or_else(|| invalid("full-sync V3 lease expiry overflow"))?;
    let expires_unix_ms = conservative_end.min(grant_end);
    if expires_unix_ms <= not_before_unix_ms
        || expires_unix_ms - not_before_unix_ms > state.policy.body().max_authorization_staleness_ms
        || expires_unix_ms - not_before_unix_ms > MAX_FULL_SYNC_PROVIDER_LEASE_MILLIS_V3
    {
        return Err(FormalError::Conflict(
            "full-sync V3 grant has no conservative provider lease interval".into(),
        ));
    }
    let registry_state_digest = digest(
        REGISTRY_DIGEST_DOMAIN_V3,
        &state.target_states,
        MAX_ENTRY_BYTES_V3,
    )?;
    Ok(FullSyncProviderLeaseBodyV3 {
        schema_version: 3,
        world: state.policy.body().world.clone(),
        authority_epoch: state.policy.body().authority_epoch,
        scope_digest: state.policy.scope_digest().to_owned(),
        policy_digest: state.policy.policy_digest().to_owned(),
        signer_generation: state.policy.body().signer_generation,
        provider_public_key: body.provider_public_key.clone(),
        provider_incarnation: body.provider_incarnation.clone(),
        lease_id: body.lease_id.clone(),
        request_id: body.request_id.clone(),
        request_nonce: body.request_nonce.clone(),
        target: body.target.clone(),
        target_generation: body.target_generation,
        expected_head: state.anchor.clone(),
        registry_state_digest,
        decision_revision: state.anchor.revision,
        clock_profile_digest: state.policy.body().clock_profile_digest.clone(),
        not_before_unix_ms,
        expires_unix_ms,
    })
}

fn event_request_id(event: &FullSyncIssuerEventV3) -> &str {
    match event {
        FullSyncIssuerEventV3::Migration(migration) => &migration.body.request_id,
        FullSyncIssuerEventV3::Management(command) => &command.body.request_id,
        FullSyncIssuerEventV3::Lease { request, .. } => &request.body.request_id,
    }
}

fn event_request_digest(event: &FullSyncIssuerEventV3) -> Result<String, FormalError> {
    digest(REQUEST_DIGEST_DOMAIN_V3, event, MAX_ENTRY_BYTES_V3)
}

fn witness_nonce_v2(observation: &SignedFullSyncIssuerObservationV2) -> (String, String, String) {
    (
        "oasis7.formal.full-sync-issuer.observation.v2".into(),
        observation.body.query.body.issuer_public_key.clone(),
        observation.body.query.body.observation_nonce.clone(),
    )
}

fn witness_nonce_v3(observation: &ReceiptObservationV3) -> (String, String, String) {
    match observation {
        ReceiptObservationV3::Migration(observation) => witness_nonce_v2(observation),
        ReceiptObservationV3::V3(observation) => (
            "oasis7.formal.full-sync-issuer.observation.v3".into(),
            observation.body.query.body.lease_signer_public_key.clone(),
            observation.body.query.body.observation_nonce.clone(),
        ),
    }
}

fn collect_v2_witness_nonces(
    entries: &[FullSyncIssuerCommittedEntryV2],
) -> Result<BTreeSet<(String, String, String)>, FormalError> {
    let mut nonces = BTreeSet::new();
    for entry in entries {
        if !nonces.insert(witness_nonce_v2(&entry.receipt.body.observation)) {
            return Err(FormalError::Conflict(
                "full-sync V2 witness observation nonce reused".into(),
            ));
        }
    }
    Ok(nonces)
}

fn ensure_next_event_capacity(state: &VerifiedFullSyncIssuerStateV3) -> Result<(), FormalError> {
    if state.v2_entries.len() + state.entries.len() >= MAX_FULL_SYNC_ISSUER_LEDGER_RECORDS_V3 {
        return Err(invalid("full-sync V3 total ledger record capacity"));
    }
    if state.encoded_bytes >= MAX_LEDGER_BYTES_V3 {
        return Err(invalid("full-sync V3 ledger byte capacity"));
    }
    Ok(())
}

fn ensure_capacity_v3(
    encoded_bytes: usize,
    record_count: usize,
    record: &FullSyncIssuerRecordV3,
) -> Result<(), FormalError> {
    if record_count >= MAX_FULL_SYNC_ISSUER_LEDGER_RECORDS_V3 {
        return Err(invalid("full-sync V3 total ledger record capacity"));
    }
    let record_bytes = encode(STORED_ENTRY_DOMAIN_V3, record, MAX_STORED_ENTRY_BYTES_V3)?.len();
    encoded_bytes
        .checked_add(record_bytes)
        .and_then(|bytes| bytes.checked_add(RECEIPT_RESERVE_BYTES_V3))
        .filter(|bytes| *bytes <= MAX_LEDGER_BYTES_V3)
        .ok_or_else(|| invalid("full-sync V3 ledger byte capacity"))?;
    Ok(())
}

fn successor_anchor_v3(
    previous: &FullSyncIssuerPreviousAnchorV3,
    event: &FullSyncIssuerEventV3,
    state: AnchorStateV3<'_>,
) -> Result<FullSyncIssuerAnchorV3, FormalError> {
    let previous_revision = match previous {
        FullSyncIssuerPreviousAnchorV3::V2(anchor) => {
            anchor.validate()?;
            if anchor != state.v2_head || anchor.scope_digest != state.policy.scope_digest() {
                return Err(invalid("full-sync V3 migration predecessor binding"));
            }
            anchor.revision
        }
        FullSyncIssuerPreviousAnchorV3::V3(anchor) => {
            anchor.validate()?;
            if anchor.scope_digest != state.policy.scope_digest()
                || anchor.policy_digest != state.policy.policy_digest()
                || anchor.signer_generation != state.policy.body().signer_generation
            {
                return Err(invalid("full-sync V3 predecessor policy binding"));
            }
            anchor.revision
        }
    };
    let revision = previous_revision
        .checked_add(1)
        .ok_or_else(|| invalid("full-sync V3 anchor revision overflow"))?;
    let state_digest = digest(
        STATE_DOMAIN_V3,
        &(
            state.v2_head,
            state.policy.policy_digest(),
            state.policy.body().signer_generation,
            state.target_states,
            state.leases,
            state.request_digests,
            state.manager_nonces,
            state.provider_nonces,
            state.lease_ids,
            state.witness_nonces,
        ),
        MAX_LEDGER_BYTES_V3,
    )?;
    let event_digest = digest(EVENT_DIGEST_DOMAIN_V3, event, MAX_ENTRY_BYTES_V3)?;
    let previous_log_digest = match previous {
        FullSyncIssuerPreviousAnchorV3::V2(anchor) => &anchor.log_digest,
        FullSyncIssuerPreviousAnchorV3::V3(anchor) => &anchor.log_digest,
    };
    let log_digest = digest(
        LOG_LINK_DOMAIN_V3,
        &(
            previous_log_digest,
            previous,
            event_digest,
            &state_digest,
            state.policy.policy_digest(),
            revision,
        ),
        MAX_ENTRY_BYTES_V3,
    )?;
    let anchor = FullSyncIssuerAnchorV3 {
        schema_version: 3,
        revision,
        scope_digest: state.policy.scope_digest().to_owned(),
        log_digest,
        state_digest,
        policy_digest: state.policy.policy_digest().to_owned(),
        signer_generation: state.policy.body().signer_generation,
    };
    anchor.validate()?;
    Ok(anchor)
}

fn anchor_at_v3_revision(
    state: &VerifiedFullSyncIssuerStateV3,
    revision: u64,
) -> Result<FullSyncIssuerAnchorV3, FormalError> {
    let first_v3_revision = state
        .v2_head
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("full-sync V3 migration revision overflow"))?;
    if revision < first_v3_revision {
        return Err(invalid("full-sync V3 minimum precedes migration"));
    }
    let index = usize::try_from(revision - first_v3_revision)
        .map_err(|_| invalid("full-sync V3 anchor revision conversion"))?;
    state
        .entries
        .get(index)
        .map(|entry| entry.record.next_anchor.clone())
        .ok_or_else(|| invalid("full-sync V3 minimum absent from complete history"))
}

pub(super) fn commit(
    prepared: PreparedFullSyncIssuerEventV3,
    receipt: SignedFullSyncIssuerReceiptV3,
) -> Result<VerifiedFullSyncIssuerDecisionV3, FormalError> {
    let record = prepared
        .record
        .ok_or_else(|| invalid("full-sync V3 idempotent decision already committed"))?;
    if receipt.body.observation != prepared.expected_observation {
        return Err(invalid("full-sync V3 receipt used a different observation"));
    }
    let mut state = prepared
        .next_state
        .ok_or_else(|| invalid("full-sync V3 prepared state unavailable"))?;
    receipt.verify_for_record(&record, &prepared.old_policy, &state.policy)?;
    let expected_previous = match state.entries.last() {
        Some(entry) => FullSyncIssuerPreviousAnchorV3::V3(entry.record.next_anchor.clone()),
        None => FullSyncIssuerPreviousAnchorV3::V2(state.v2_head.clone()),
    };
    if record.previous_anchor != expected_previous {
        return Err(FormalError::Conflict(
            "full-sync V3 prepared predecessor changed".into(),
        ));
    }
    let entry = FullSyncIssuerCommittedEntryV3 {
        record: record.clone(),
        receipt,
    };
    let stored_bytes = encode(STORED_ENTRY_DOMAIN_V3, &entry, MAX_STORED_ENTRY_BYTES_V3)?.len();
    let encoded_bytes = state
        .encoded_bytes
        .checked_add(stored_bytes)
        .filter(|bytes| *bytes <= MAX_LEDGER_BYTES_V3)
        .ok_or_else(|| invalid("full-sync V3 ledger byte capacity"))?;
    let request_id = record.request_id().to_owned();
    let index = state.entries.len();
    if state.request_ids.get(&request_id) != Some(&FullSyncIssuerRequestLocationV3::V3(index)) {
        return Err(FormalError::Conflict(
            "full-sync V3 prepared request index changed".into(),
        ));
    }
    state.anchor = record.next_anchor.clone();
    state.encoded_bytes = encoded_bytes;
    state.entries.push(entry.clone());
    Ok(VerifiedFullSyncIssuerDecisionV3 { entry, state })
}

pub(super) fn recover_migration_resume(
    old_policy: VerifiedFullSyncIssuerPolicyV2,
    new_policy: VerifiedFullSyncIssuerPolicyV3,
    expected_query: &SignedFullSyncIssuerMigrationResumeQueryV3,
    bundle: FullSyncIssuerMigrationResumeBundleV3,
) -> Result<FullSyncIssuerMigrationResumeResultV3, FormalError> {
    if bundle.observation.body.query != *expected_query {
        return Err(invalid(
            "full-sync migration-resume reply does not match the fresh query",
        ));
    }
    let query = &bundle.observation.body.query.body;
    let migration = &query.exact_migration;
    let v2_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        old_policy.clone(),
        bundle.history.v2_entries.clone(),
    )?;
    let (migration_policy, expected_head) =
        v2_state_at_migration_head(&v2_state, &migration.body.expected_v2_head)?;
    let verified_new_policy = VerifiedFullSyncIssuerPolicyV3::verify(
        new_policy.signed(),
        new_policy.trust(),
        &migration_policy,
    )?;
    if verified_new_policy.policy_digest() != new_policy.policy_digest() {
        return Err(invalid("full-sync V3 resume policy verification changed"));
    }
    expected_query.verify(&migration_policy, &verified_new_policy)?;
    bundle
        .observation
        .verify(&migration_policy, &verified_new_policy)?;
    query.original_v2_query.verify(&migration_policy)?;
    migration.verify(&migration_policy, &verified_new_policy)?;
    if query.original_v2_query.body.minimum_anchor != expected_head {
        return Err(invalid(
            "full-sync resume original V2 minimum is not qualified",
        ));
    }

    match &bundle.observation.body.current_mode_head {
        FullSyncIssuerWitnessModeHeadV3::V2(current_anchor) => {
            if !bundle.history.v3_entries.is_empty() || bundle.original_migration_receipt.is_some()
            {
                return Err(invalid("full-sync V2 mode returned V3 migration material"));
            }
            if current_anchor != v2_state.anchor() {
                return Err(FormalError::Rollback(
                    "full-sync resume V2 history does not close current mode head".into(),
                ));
            }
            let request_used = v2_state.requests.contains_key(&migration.body.request_id);
            Ok(FullSyncIssuerMigrationResumeResultV3::NotCommittedV2 {
                current_state: Box::new(v2_state),
                current_anchor: current_anchor.clone(),
                migration_head_conflict: request_used
                    || *current_anchor != migration.body.expected_v2_head,
            })
        }
        FullSyncIssuerWitnessModeHeadV3::V3(current_anchor) => {
            let history = bundle.history;
            let state = reconstruct_history(old_policy, verified_new_policy, history.clone())?;
            if &state.anchor != current_anchor {
                return Err(FormalError::Rollback(
                    "full-sync resume V3 history does not close current mode head".into(),
                ));
            }
            let migration_entry = state
                .entries
                .first()
                .ok_or_else(|| invalid("full-sync resume V3 migration entry absent"))?;
            if !matches!(
                &migration_entry.record.event,
                FullSyncIssuerEventV3::Migration(found) if found.as_ref() == migration
            ) {
                return Err(invalid(
                    "full-sync resume requested migration differs from history",
                ));
            }
            let receipt = bundle
                .original_migration_receipt
                .ok_or_else(|| invalid("full-sync committed migration receipt absent"))?;
            if receipt != migration_entry.receipt {
                return Err(invalid("full-sync resume migration receipt mismatch"));
            }
            Ok(FullSyncIssuerMigrationResumeResultV3::Committed(Box::new(
                FullSyncIssuerCommittedMigrationResumeV3 {
                    history,
                    migration_entry: migration_entry.clone(),
                    original_receipt: receipt,
                    anchor: migration_entry.record.next_anchor.clone(),
                },
            )))
        }
    }
}

fn v2_state_at_migration_head(
    state: &VerifiedFullSyncIssuerStateV2,
    expected: &FullSyncIssuerAnchorV2,
) -> Result<(VerifiedFullSyncIssuerPolicyV2, FullSyncIssuerAnchorV2), FormalError> {
    expected.validate()?;
    if expected.scope_digest != state.anchor().scope_digest {
        return Err(invalid("full-sync resume migration V2 scope"));
    }
    let revision = usize::try_from(expected.revision)
        .map_err(|_| invalid("full-sync resume migration revision conversion"))?;
    let prefix = state
        .entries
        .get(..revision)
        .ok_or_else(|| invalid("full-sync resume migration head outside V2 history"))?;
    let prefix_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        state.configured_policy.clone(),
        prefix.to_vec(),
    )?;
    if prefix_state.anchor() != expected {
        return Err(invalid(
            "full-sync resume migration head is not a V2 prefix",
        ));
    }
    let active = match prefix_state.active_policy.clone() {
        Some(policy) => policy,
        None if prefix_state.anchor().revision == 0 => prefix_state.configured_policy.clone(),
        None => {
            return Err(invalid(
                "full-sync resume migration has no active V2 policy",
            ));
        }
    };
    Ok((active, expected.clone()))
}

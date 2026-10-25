//! Non-serving recovery proofs for uncertain V2-to-V3 migrations.
use super::super::{
    FormalError, FullSyncIssuerAnchorV2, VerifiedFullSyncIssuerPolicyV2,
    VerifiedFullSyncIssuerStateV2, invalid,
};
use super::reducer::reconstruct_history;
use super::state::{
    FullSyncIssuerCommittedMigrationResumeV3, FullSyncIssuerMigrationResumeResultV3,
    VerifiedFullSyncIssuerMigrationConflictV3,
};
use super::types::{FullSyncIssuerEventV3, VerifiedFullSyncIssuerPolicyV3};
use super::witness::{
    FullSyncIssuerMigrationResumeBundleV3, FullSyncIssuerWitnessModeHeadV3,
    SignedFullSyncIssuerMigrationResumeQueryV3,
};

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
    let query = &expected_query.body;
    let migration = &query.exact_migration;
    let v2_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        old_policy.clone(),
        bundle.history.v2_entries.clone(),
    )?;
    let (migration_policy, expected_head) =
        v2_state_at_migration_head(&v2_state, &migration.body.expected_v2_head)?;
    let verified_new_policy = VerifiedFullSyncIssuerPolicyV3::verify(
        new_policy.signed(),
        old_policy.trust(),
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

    match bundle.observation.body.current_mode_head.clone() {
        FullSyncIssuerWitnessModeHeadV3::V2(current_anchor) => {
            if !bundle.history.v3_entries.is_empty() || bundle.original_migration_receipt.is_some()
            {
                return Err(invalid("full-sync V2 mode returned V3 migration material"));
            }
            if &current_anchor != v2_state.anchor() {
                return Err(FormalError::Rollback(
                    "full-sync resume V2 history does not close current mode head".into(),
                ));
            }
            let request_used = v2_state.requests.contains_key(&migration.body.request_id);
            Ok(FullSyncIssuerMigrationResumeResultV3::NotCommittedV2 {
                current_state: Box::new(v2_state),
                current_anchor: current_anchor.clone(),
                migration_head_conflict: request_used
                    || current_anchor != migration.body.expected_v2_head,
            })
        }
        FullSyncIssuerWitnessModeHeadV3::V3(current_anchor) => recover_v3_mode(
            old_policy,
            verified_new_policy,
            migration,
            &current_anchor,
            bundle,
        ),
    }
}

fn recover_v3_mode(
    old_policy: VerifiedFullSyncIssuerPolicyV2,
    requested_policy: VerifiedFullSyncIssuerPolicyV3,
    requested_migration: &super::types::SignedFullSyncIssuerMigrationV3,
    current_anchor: &super::types::FullSyncIssuerAnchorV3,
    bundle: FullSyncIssuerMigrationResumeBundleV3,
) -> Result<FullSyncIssuerMigrationResumeResultV3, FormalError> {
    let history = bundle.history;
    let migration_entry = history
        .v3_entries
        .first()
        .ok_or_else(|| invalid("full-sync resume V3 migration entry absent"))?;
    let committed_migration = match &migration_entry.record.event {
        FullSyncIssuerEventV3::Migration(migration) => migration.as_ref(),
        _ => return Err(invalid("full-sync resume V3 history lacks migration")),
    };

    if committed_migration == requested_migration {
        let state = reconstruct_history(old_policy, requested_policy, history.clone())?;
        if &state.anchor != current_anchor {
            return Err(FormalError::Rollback(
                "full-sync resume V3 history does not close current mode head".into(),
            ));
        }
        let receipt = bundle
            .original_migration_receipt
            .ok_or_else(|| invalid("full-sync committed migration receipt absent"))?;
        if receipt != migration_entry.receipt {
            return Err(invalid("full-sync resume migration receipt mismatch"));
        }
        let committed_entry = migration_entry.clone();
        let anchor = committed_entry.record.next_anchor.clone();
        return Ok(FullSyncIssuerMigrationResumeResultV3::Committed(Box::new(
            FullSyncIssuerCommittedMigrationResumeV3 {
                history,
                migration_entry: committed_entry,
                original_receipt: receipt,
                anchor,
            },
        )));
    }

    if committed_migration.body.request_id == requested_migration.body.request_id {
        return Err(FormalError::Conflict(
            "full-sync migration request identity was used by another event".into(),
        ));
    }
    if bundle.original_migration_receipt.is_some() {
        return Err(invalid(
            "full-sync competing migration returned the requested receipt",
        ));
    }

    // The winner's policy is authenticated independently under the original
    // activation trust and the V2 manager active at its own exact predecessor.
    let v2_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        old_policy.clone(),
        history.v2_entries.clone(),
    )?;
    let (winner_v2_policy, _) =
        v2_state_at_migration_head(&v2_state, &committed_migration.body.expected_v2_head)?;
    let winner_policy = VerifiedFullSyncIssuerPolicyV3::verify(
        &committed_migration.body.new_policy,
        old_policy.trust(),
        &winner_v2_policy,
    )?;
    let state = reconstruct_history(old_policy, winner_policy.clone(), history.clone())?;
    if &state.anchor != current_anchor {
        return Err(FormalError::Rollback(
            "full-sync competing V3 history does not close current mode head".into(),
        ));
    }
    if state
        .request_ids
        .contains_key(&requested_migration.body.request_id)
    {
        return Err(FormalError::Conflict(
            "full-sync migration request identity is present in committed history".into(),
        ));
    }

    let migration_entry = migration_entry.clone();
    Ok(
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV3MigrationConflict(Box::new(
            VerifiedFullSyncIssuerMigrationConflictV3::new(
                history,
                current_anchor.clone(),
                migration_entry,
                winner_policy,
            ),
        )),
    )
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

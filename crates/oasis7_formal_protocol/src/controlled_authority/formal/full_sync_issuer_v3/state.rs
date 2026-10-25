//! V3 reducer API. Candidate construction and replay are deliberately separate
//! from the witness CAS and the durable adapter.
use super::super::{
    FormalError, FullSyncIssuerAnchorV2, FullSyncIssuerCommittedEntryV2, FullSyncIssuerTargetV2,
    SignedFullSyncIssuerObservationV2, VerifiedFullSyncIssuerPolicyV2,
    VerifiedFullSyncIssuerStateV2,
};
use super::types::{
    FullSyncIssuerAnchorV3, FullSyncIssuerCommittedEntryV3, FullSyncIssuerPreviousAnchorV3,
    FullSyncIssuerReceiptObservationV3, FullSyncIssuerRecordV3, FullSyncIssuerTargetLeaseV3,
    FullSyncIssuerTargetStateV3, FullSyncProviderLeaseStateV3, SignedFullSyncIssuerCommandV3,
    SignedFullSyncIssuerMigrationV3, SignedFullSyncIssuerReceiptV3,
    SignedFullSyncProviderLeaseRequestV3, VerifiedFullSyncIssuerPolicyV3,
};
use super::witness::{
    FullSyncIssuerHistoryV3, FullSyncIssuerMigrationResumeBundleV3,
    SignedFullSyncIssuerMigrationResumeQueryV3, SignedFullSyncIssuerObservationV3,
};
use ed25519_dalek::SigningKey;
use std::collections::{BTreeMap, BTreeSet};

/// A validated, complete retained log: immutable V2 prefix plus the V3 suffix.
#[derive(Debug, Clone)]
pub struct VerifiedFullSyncIssuerStateV3 {
    pub(super) v2_policy: VerifiedFullSyncIssuerPolicyV2,
    pub(super) policy: VerifiedFullSyncIssuerPolicyV3,
    pub(super) v2_entries: Vec<FullSyncIssuerCommittedEntryV2>,
    pub(super) entries: Vec<FullSyncIssuerCommittedEntryV3>,
    pub(super) anchor: FullSyncIssuerAnchorV3,
    pub(super) v2_head: FullSyncIssuerAnchorV2,
    pub(super) target_states: BTreeMap<FullSyncIssuerTargetV2, FullSyncIssuerTargetStateV3>,
    pub(super) leases: BTreeMap<FullSyncIssuerTargetLeaseV3, FullSyncProviderLeaseStateV3>,
    pub(super) request_ids: BTreeMap<String, FullSyncIssuerRequestLocationV3>,
    pub(super) request_digests: BTreeMap<String, String>,
    pub(super) manager_nonces: BTreeSet<(String, String)>,
    pub(super) provider_nonces: BTreeSet<(String, String)>,
    pub(super) lease_ids: BTreeSet<(String, String, String)>,
    pub(super) witness_nonces: BTreeSet<(String, String, String)>,
    pub(super) encoded_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FullSyncIssuerRequestLocationV3 {
    V2(usize),
    V3(usize),
}

impl VerifiedFullSyncIssuerStateV2 {
    /// Prepare the one-way V2→V3 transition against the exact current V2 head.
    pub fn prepare_migration_v3(
        &self,
        new_policy: VerifiedFullSyncIssuerPolicyV3,
        migration: SignedFullSyncIssuerMigrationV3,
        observation: &SignedFullSyncIssuerObservationV2,
    ) -> Result<PreparedFullSyncIssuerEventV3, FormalError> {
        super::reducer::prepare_migration(self, new_policy, migration, observation)
    }
}

impl VerifiedFullSyncIssuerStateV3 {
    /// Reconstruct both schema generations from a nonce-bound complete witness
    /// response. `v3_entries` begins with the unique migration event.
    pub fn from_witness_observation(
        v2_policy: VerifiedFullSyncIssuerPolicyV2,
        v3_policy: VerifiedFullSyncIssuerPolicyV3,
        history: FullSyncIssuerHistoryV3,
        observation: &SignedFullSyncIssuerObservationV3,
    ) -> Result<Self, FormalError> {
        super::reducer::reconstruct(v2_policy, v3_policy, history, observation)
    }

    pub fn anchor(&self) -> &FullSyncIssuerAnchorV3 {
        &self.anchor
    }

    pub fn policy(&self) -> &VerifiedFullSyncIssuerPolicyV3 {
        &self.policy
    }

    pub fn v2_policy(&self) -> &VerifiedFullSyncIssuerPolicyV2 {
        &self.v2_policy
    }

    pub fn v2_prefix(&self) -> &[FullSyncIssuerCommittedEntryV2] {
        &self.v2_entries
    }

    pub fn entries(&self) -> &[FullSyncIssuerCommittedEntryV3] {
        &self.entries
    }

    pub fn target_state(
        &self,
        target: &FullSyncIssuerTargetV2,
    ) -> Option<&FullSyncIssuerTargetStateV3> {
        self.target_states.get(target)
    }

    pub fn leases(&self) -> &BTreeMap<FullSyncIssuerTargetLeaseV3, FullSyncProviderLeaseStateV3> {
        &self.leases
    }

    /// Prepare a V3 management event. The witness observation must be at the
    /// exact current anchor; a stale request is not automatically rebased.
    pub fn prepare_management(
        &self,
        command: SignedFullSyncIssuerCommandV3,
        observation: &SignedFullSyncIssuerObservationV3,
    ) -> Result<PreparedFullSyncIssuerEventV3, FormalError> {
        super::reducer::prepare_management(self, command, observation)
    }

    /// Prepare a provider lease event. Existing exact request identities must
    /// be returned before checking current target activity/time, so retries
    /// recover the historical decision without renewing or authorizing it.
    pub fn prepare_lease(
        &self,
        request: SignedFullSyncProviderLeaseRequestV3,
        observation: &SignedFullSyncIssuerObservationV3,
        lease_signer: &SigningKey,
    ) -> Result<PreparedFullSyncIssuerEventV3, FormalError> {
        super::reducer::prepare_lease(self, request, observation, lease_signer)
    }
}

/// Result of reconciling an uncertain migration using a fresh, nonce-bound
/// mode observation and complete retained history. The committed branch does
/// not return a serving V3 state; deployment must pin the anchor and reopen.
#[derive(Debug, Clone)]
pub struct FullSyncIssuerCommittedMigrationResumeV3 {
    pub history: FullSyncIssuerHistoryV3,
    pub migration_entry: FullSyncIssuerCommittedEntryV3,
    pub original_receipt: SignedFullSyncIssuerReceiptV3,
    pub anchor: FullSyncIssuerAnchorV3,
}

/// Non-serving proof that a different V3 migration won while a requested
/// migration remained absent from the complete, verified history.
#[derive(Debug, Clone)]
pub struct VerifiedFullSyncIssuerMigrationConflictV3 {
    history: FullSyncIssuerHistoryV3,
    current_anchor: FullSyncIssuerAnchorV3,
    migration_entry: FullSyncIssuerCommittedEntryV3,
    policy: VerifiedFullSyncIssuerPolicyV3,
}

impl VerifiedFullSyncIssuerMigrationConflictV3 {
    pub(super) fn new(
        history: FullSyncIssuerHistoryV3,
        current_anchor: FullSyncIssuerAnchorV3,
        migration_entry: FullSyncIssuerCommittedEntryV3,
        policy: VerifiedFullSyncIssuerPolicyV3,
    ) -> Self {
        Self {
            history,
            current_anchor,
            migration_entry,
            policy,
        }
    }

    pub fn history(&self) -> &FullSyncIssuerHistoryV3 {
        &self.history
    }

    pub fn current_anchor(&self) -> &FullSyncIssuerAnchorV3 {
        &self.current_anchor
    }

    pub fn migration_entry(&self) -> &FullSyncIssuerCommittedEntryV3 {
        &self.migration_entry
    }

    pub fn policy(&self) -> &VerifiedFullSyncIssuerPolicyV3 {
        &self.policy
    }
}

#[derive(Debug, Clone)]
pub enum FullSyncIssuerMigrationResumeResultV3 {
    Committed(Box<FullSyncIssuerCommittedMigrationResumeV3>),
    NotCommittedV3MigrationConflict(Box<VerifiedFullSyncIssuerMigrationConflictV3>),
    NotCommittedV2 {
        current_state: Box<VerifiedFullSyncIssuerStateV2>,
        current_anchor: FullSyncIssuerAnchorV2,
        migration_head_conflict: bool,
    },
}

/// Recover an uncertain migration only from the response to `expected_query`.
/// Callers must pass the fresh signed query they sent to the witness.
pub fn recover_migration_resume_v3(
    old_policy: VerifiedFullSyncIssuerPolicyV2,
    new_policy: VerifiedFullSyncIssuerPolicyV3,
    expected_query: &SignedFullSyncIssuerMigrationResumeQueryV3,
    bundle: FullSyncIssuerMigrationResumeBundleV3,
) -> Result<FullSyncIssuerMigrationResumeResultV3, FormalError> {
    super::migration_resume::recover_migration_resume(
        old_policy,
        new_policy,
        expected_query,
        bundle,
    )
}

/// Prepared event bytes are not committed authorization. Only `commit` with a
/// matching witness receipt yields a committed decision.
#[derive(Debug, Clone)]
pub struct PreparedFullSyncIssuerEventV3 {
    pub(super) record: Option<FullSyncIssuerRecordV3>,
    pub(super) existing: Option<FullSyncIssuerCommittedEntryV3>,
    pub(super) next_state: Option<VerifiedFullSyncIssuerStateV3>,
    pub(super) old_policy: VerifiedFullSyncIssuerPolicyV2,
    pub(super) expected_observation: FullSyncIssuerReceiptObservationV3,
}

impl PreparedFullSyncIssuerEventV3 {
    pub fn record(&self) -> Option<&FullSyncIssuerRecordV3> {
        self.record.as_ref()
    }

    pub fn existing(&self) -> Option<&FullSyncIssuerCommittedEntryV3> {
        self.existing.as_ref()
    }

    pub fn previous(&self) -> Option<&FullSyncIssuerPreviousAnchorV3> {
        self.record.as_ref().map(|record| &record.previous_anchor)
    }

    pub fn next(&self) -> Option<&FullSyncIssuerAnchorV3> {
        self.record.as_ref().map(|record| &record.next_anchor)
    }

    pub fn commit(
        self,
        receipt: SignedFullSyncIssuerReceiptV3,
    ) -> Result<VerifiedFullSyncIssuerDecisionV3, FormalError> {
        super::reducer::commit(self, receipt)
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedFullSyncIssuerDecisionV3 {
    pub(super) entry: FullSyncIssuerCommittedEntryV3,
    pub(super) state: VerifiedFullSyncIssuerStateV3,
}

impl VerifiedFullSyncIssuerDecisionV3 {
    pub fn record(&self) -> &FullSyncIssuerRecordV3 {
        &self.entry.record
    }

    pub fn receipt(&self) -> &SignedFullSyncIssuerReceiptV3 {
        &self.entry.receipt
    }

    pub fn entry(&self) -> &FullSyncIssuerCommittedEntryV3 {
        &self.entry
    }

    pub fn state(&self) -> &VerifiedFullSyncIssuerStateV3 {
        &self.state
    }
}

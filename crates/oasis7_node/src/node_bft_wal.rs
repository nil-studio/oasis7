use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use oasis7_consensus::tendermint_v1::{
    BFT_MAX_ROUNDS_PER_HEIGHT_V1, BftHeightProposerScheduleV1, BftProposalIntentV1,
    BftSigningJournalEventV1, BftSigningJournalV1, BftVoteIntentV1, VerifiedBftCommitCertificateV1,
};
use oasis7_proto::distributed_bft::{
    BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1, BftCommitCertificateV1, BftProposalV1, BftScopeV1,
    BftVoteV1,
};
use serde::{Deserialize, Serialize};

use crate::NodeError;

const BFT_WAL_SCHEMA_VERSION: u32 = 4;
const BFT_WAL_FILE: &str = "tendermint-v1.wal.cbor";
const BFT_WAL_LOCK_FILE: &str = "tendermint-v1.wal.lock";
const BFT_WAL_ANCHOR_FILE: &str = "tendermint-v1.anchor.cbor";
pub(super) const BFT_WAL_PROTECTED_FILE: &str = "tendermint-v1.protected.cbor";
const BFT_WAL_PROTECTED_MARKER_FILE: &str = "tendermint-v1.protected-mode.cbor";

#[path = "node_bft_wal/protected.rs"]
mod protected;
use protected::ProtectedWal;

pub(super) type BftWalStateV1 = BftSigningJournalV1;
pub(super) type BftWalEventV1 = BftSigningJournalEventV1;

pub(super) struct BftWalStore {
    directory: PathBuf,
    state: BftWalStateV1,
    local_validator_id: String,
    local_signing_public_key: String,
    protected: Option<ProtectedWal>,
    _lock: File,
}

impl BftWalStore {
    /// Open signer state for cold recovery using only the operator-configured
    /// genesis schedule as the fixed identity anchor. The returned WAL state
    /// is untrusted until the caller has replayed complete commit history and
    /// calls `validate_schedule` with the derived schedule before signing.
    pub(super) fn open_for_recovery(
        directory: &Path,
        genesis_schedule: &BftHeightProposerScheduleV1,
        local_validator_id: &str,
        local_signing_public_key: &str,
        has_committed_history: bool,
    ) -> Result<Self, NodeError> {
        if genesis_schedule.height() != 1
            || genesis_schedule.parent_commit_ref() != genesis_schedule.scope().genesis_hash
        {
            return Err(NodeError::Consensus {
                reason: "BFT recovery must be anchored to the configured genesis schedule".into(),
            });
        }
        Self::open(
            directory,
            genesis_schedule,
            local_validator_id,
            local_signing_public_key,
            has_committed_history,
        )
    }

    pub(super) fn open(
        directory: &Path,
        schedule: &BftHeightProposerScheduleV1,
        local_validator_id: &str,
        local_signing_public_key: &str,
        has_committed_history: bool,
    ) -> Result<Self, NodeError> {
        if local_validator_id.trim().is_empty()
            || local_signing_public_key.len() != 64
            || !local_signing_public_key
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(NodeError::Consensus {
                reason: "BFT WAL requires a canonical local validator identity and public key"
                    .into(),
            });
        }
        ensure_directory(directory)?;
        if path_exists_without_following(directory.join(BFT_WAL_PROTECTED_MARKER_FILE).as_path())
            || path_exists_without_following(directory.join(BFT_WAL_PROTECTED_FILE).as_path())
        {
            return Err(NodeError::Consensus {
                reason: "protected BFT signer state is present; refusing local WAL downgrade"
                    .into(),
            });
        }
        let lock = acquire_wal_lock(directory)?;

        let state_path = directory.join(BFT_WAL_FILE);
        let anchor_path = directory.join(BFT_WAL_ANCHOR_FILE);
        let anchor_exists = path_exists_without_following(anchor_path.as_path());
        let state_exists = path_exists_without_following(state_path.as_path());
        if anchor_exists {
            validate_anchor(
                anchor_path.as_path(),
                schedule,
                local_validator_id,
                local_signing_public_key,
            )?;
        } else if state_exists || has_committed_history {
            return Err(NodeError::Consensus {
                reason: "BFT signer WAL anchor is missing while state/history exists; preserve evidence and stop".into(),
            });
        }
        let state = if state_exists {
            read_state(state_path.as_path())?
        } else {
            if has_committed_history || anchor_exists {
                return Err(NodeError::Consensus {
                    reason:
                        "BFT signer WAL is missing after committed history; preserve state and stop"
                            .into(),
                });
            }
            if schedule.height() != 1
                || schedule.parent_commit_ref() != schedule.scope().genesis_hash
            {
                return Err(NodeError::Consensus {
                    reason: "BFT WAL may only be initialized from the configured genesis schedule"
                        .into(),
                });
            }
            persist_anchor(
                directory,
                schedule,
                local_validator_id,
                local_signing_public_key,
            )?;
            let state = state_for_schedule(schedule, local_validator_id, local_signing_public_key);
            persist_state(directory, &state)?;
            state
        };
        validate_local_identity(
            &state,
            schedule,
            local_validator_id,
            local_signing_public_key,
        )?;
        validate_local_wal_events(&state)?;
        Ok(Self {
            directory: directory.to_path_buf(),
            state,
            local_validator_id: local_validator_id.to_string(),
            local_signing_public_key: local_signing_public_key.to_string(),
            protected: None,
            _lock: lock,
        })
    }

    pub(super) fn state(&self) -> &BftWalStateV1 {
        &self.state
    }

    pub(super) fn validate_schedule(
        &self,
        schedule: &BftHeightProposerScheduleV1,
    ) -> Result<(), NodeError> {
        validate_schedule_binding(&self.state, schedule)
    }

    /// Persist the first local clock sample used to initialize this height's
    /// phase timers. Recovery must reuse this value rather than restarting
    /// timeout windows from process startup time.
    pub(super) fn ensure_height_started_at(&mut self, now_ms: i64) -> Result<i64, NodeError> {
        if self.protected.is_some() {
            return protected::start_height_timer(self);
        }
        if let Some(started_at_ms) = self.state.height_started_at_ms {
            return Ok(started_at_ms);
        }
        if now_ms < 0 {
            return Err(NodeError::Consensus {
                reason: "BFT height start time cannot be negative".into(),
            });
        }
        let mut next = self.state.clone();
        next.height_started_at_ms = Some(now_ms);
        self.persist(next)?;
        Ok(now_ms)
    }

    pub(super) fn append_event(&mut self, event: BftWalEventV1) -> Result<(), NodeError> {
        if matches!(
            &event,
            BftWalEventV1::Proposal { local: true, .. } | BftWalEventV1::Vote { local: true, .. }
        ) {
            return Err(NodeError::Consensus {
                reason: "local BFT signatures must pass through a durable intent reservation"
                    .into(),
            });
        }
        if self.state.pending_commit.is_some() {
            return Err(NodeError::Consensus {
                reason: "BFT WAL already has a pending commit; height events are closed".into(),
            });
        }
        validate_event_time(&self.state, &event)?;
        if self.state.events.iter().any(|existing| existing == &event) {
            return Ok(());
        }
        let mut next = self.state.clone();
        next.events.push(event);
        self.persist(next)
    }

    pub(super) fn reserve_proposal_intent(
        &mut self,
        intent: BftProposalIntentV1,
    ) -> Result<(), NodeError> {
        if self.state.pending_commit.is_some() {
            return Err(NodeError::Consensus {
                reason: "BFT WAL has a pending commit; proposal signing is closed".into(),
            });
        }
        require_height_start_time(&self.state)?;
        validate_proposal_intent_for_state(
            &self.state,
            &intent,
            self.local_validator_id.as_str(),
            self.local_signing_public_key.as_str(),
        )?;
        if let Some(existing) = self
            .state
            .events
            .iter()
            .filter_map(|event| local_proposal_event(event, self.local_validator_id.as_str()))
            .find(|proposal| {
                proposal.header.round == intent.header.round
                    && proposal.header.proposer_id == intent.header.proposer_id
            })
        {
            return if proposal_matches_intent(existing, &intent) {
                Ok(())
            } else {
                Err(NodeError::Consensus {
                    reason: "BFT proposal signing slot conflicts with a completed WAL decision"
                        .into(),
                })
            };
        }
        let mut next = self.state.clone();
        match next.pending_proposal_intent.as_ref() {
            Some(existing) if existing == &intent => return Ok(()),
            Some(_) => {
                return Err(NodeError::Consensus {
                    reason: "BFT WAL contains a conflicting pending proposal signing intent".into(),
                });
            }
            None => next.pending_proposal_intent = Some(intent),
        }
        self.persist(next)
    }

    pub(super) fn append_local_proposal(
        &mut self,
        intent: &BftProposalIntentV1,
        proposal: BftProposalV1,
        now_ms: i64,
    ) -> Result<(), NodeError> {
        if self.protected.is_some() {
            return protected::accept_signed_proposal(self, intent, proposal, now_ms);
        }
        if self.state.pending_commit.is_some() {
            return Err(NodeError::Consensus {
                reason: "BFT WAL has a pending commit; proposal signing is closed".into(),
            });
        }
        require_height_start_time(&self.state)?;
        validate_proposal_intent_for_state(
            &self.state,
            intent,
            self.local_validator_id.as_str(),
            self.local_signing_public_key.as_str(),
        )?;
        let expected_header = intent.header.clone();
        let mut actual_header = proposal.header.clone();
        actual_header.signature_hex.clear();
        if self.state.pending_proposal_intent.as_ref() != Some(intent)
            || !expected_header.signature_hex.is_empty()
            || actual_header != expected_header
            || proposal.block != intent.block
            || proposal.valid_round_evidence != intent.valid_round_evidence
            || proposal.header.verify_signature().is_err()
            || proposal.validate_binding().is_err()
            || proposal.header.proposer_id != self.local_validator_id
            || proposal.header.public_key_hex != self.local_signing_public_key
        {
            if let Some(existing) = self
                .state
                .events
                .iter()
                .filter_map(|event| local_proposal_event(event, self.local_validator_id.as_str()))
                .find(|existing| {
                    existing.header.round == proposal.header.round
                        && existing.header.proposer_id == proposal.header.proposer_id
                })
                && existing == &proposal
                && proposal_matches_intent(existing, intent)
            {
                return Ok(());
            }
            return Err(NodeError::Consensus {
                reason: "BFT signed proposal does not match its durable intent".into(),
            });
        }
        validate_event_time(
            &self.state,
            &BftWalEventV1::Proposal {
                proposal: Box::new(proposal.clone()),
                local: true,
                now_ms,
            },
        )?;
        let mut next = self.state.clone();
        next.events.push(BftWalEventV1::Proposal {
            proposal: Box::new(proposal),
            local: true,
            now_ms,
        });
        next.pending_proposal_intent = None;
        self.persist(next)
    }

    pub(super) fn reserve_vote_intent(&mut self, intent: BftVoteIntentV1) -> Result<(), NodeError> {
        if self.state.pending_commit.is_some() {
            return Err(NodeError::Consensus {
                reason: "BFT WAL has a pending commit; vote signing is closed".into(),
            });
        }
        require_height_start_time(&self.state)?;
        validate_vote_intent_for_state(&self.state, &intent, self.local_validator_id.as_str())?;
        if let Some(existing) = self
            .state
            .events
            .iter()
            .filter_map(|event| local_vote_event(event, self.local_validator_id.as_str()))
            .find(|vote| {
                vote.round == intent.round
                    && vote.phase == intent.phase
                    && vote.validator_id == intent.validator_id
            })
        {
            return if vote_matches_intent(existing, &intent) {
                Ok(())
            } else {
                Err(NodeError::Consensus {
                    reason: "BFT vote signing slot conflicts with a completed WAL decision".into(),
                })
            };
        }
        let mut next = self.state.clone();
        match next.pending_vote_intent.as_ref() {
            Some(existing) if existing == &intent => return Ok(()),
            Some(_) => {
                return Err(NodeError::Consensus {
                    reason: "BFT WAL contains a conflicting pending signing intent".into(),
                });
            }
            None => next.pending_vote_intent = Some(intent),
        }
        self.persist(next)
    }

    pub(super) fn append_local_vote(
        &mut self,
        intent: &BftVoteIntentV1,
        vote: BftVoteV1,
        now_ms: i64,
    ) -> Result<(), NodeError> {
        if self.protected.is_some() {
            return protected::accept_signed_vote(self, intent, vote, now_ms);
        }
        if self.state.pending_commit.is_some() {
            return Err(NodeError::Consensus {
                reason: "BFT WAL has a pending commit; vote signing is closed".into(),
            });
        }
        require_height_start_time(&self.state)?;
        validate_vote_intent_for_state(&self.state, intent, self.local_validator_id.as_str())?;
        if self.state.pending_vote_intent.as_ref() != Some(intent)
            || vote.height != intent.height
            || vote.round != intent.round
            || vote.phase != intent.phase
            || vote.validator_id != intent.validator_id
            || vote.value != intent.value
            || vote.validator_id != self.local_validator_id
            || vote.public_key_hex != self.local_signing_public_key
            || vote.verify_signature().is_err()
        {
            if let Some(existing) = self
                .state
                .events
                .iter()
                .filter_map(|event| local_vote_event(event, self.local_validator_id.as_str()))
                .find(|existing| {
                    existing.round == vote.round
                        && existing.phase == vote.phase
                        && existing.validator_id == vote.validator_id
                })
                && existing == &vote
                && vote_matches_intent(existing, intent)
            {
                return Ok(());
            }
            return Err(NodeError::Consensus {
                reason: "BFT signed vote does not match its durable intent".into(),
            });
        }
        validate_event_time(
            &self.state,
            &BftWalEventV1::Vote {
                vote: Box::new(vote.clone()),
                local: true,
                now_ms,
            },
        )?;
        let mut next = self.state.clone();
        next.events.push(BftWalEventV1::Vote {
            vote: Box::new(vote),
            local: true,
            now_ms,
        });
        next.pending_vote_intent = None;
        self.persist(next)
    }

    pub(super) fn reserve_commit(
        &mut self,
        verified: &VerifiedBftCommitCertificateV1,
    ) -> Result<(), NodeError> {
        require_height_start_time(&self.state)?;
        validate_schedule_binding(&self.state, verified.schedule())?;
        let certificate = verified.certificate().clone();
        let mut next = self.state.clone();
        match next.pending_commit.as_ref() {
            Some(existing) if existing == &certificate => return Ok(()),
            Some(_) => {
                return Err(NodeError::Consensus {
                    reason: "BFT WAL contains a conflicting pending commit certificate".into(),
                });
            }
            None => next.pending_commit = Some(certificate),
        }
        self.persist(next)
    }

    pub(super) fn complete_commit(
        &mut self,
        verified: &VerifiedBftCommitCertificateV1,
    ) -> Result<(), NodeError> {
        validate_schedule_binding(&self.state, verified.schedule())?;
        if self.state.pending_commit.as_ref() != Some(verified.certificate()) {
            return Err(NodeError::Consensus {
                reason: "BFT WAL advances only after the exact verified pending certificate".into(),
            });
        }
        self.persist(state_for_schedule(
            verified.next_height_schedule(),
            self.local_validator_id.as_str(),
            self.local_signing_public_key.as_str(),
        ))
    }

    /// Reconcile the only safe crash window: the hook already committed the
    /// exact pending QC, and independent history replay proved its next-height
    /// schedule. Caller must not invoke this with a merely syntactic QC.
    pub(super) fn complete_history_commit(
        &mut self,
        verified: &VerifiedBftCommitCertificateV1,
    ) -> Result<(), NodeError> {
        validate_schedule_binding(&self.state, verified.schedule())?;
        let schedule = verified.next_height_schedule();
        if self.state.height.checked_add(1) != Some(schedule.height())
            || self.state.pending_commit.as_ref() != Some(verified.certificate())
            || verified.certificate().proposal.block.height != self.state.height
            || verified.certificate().proposal.block.parent_commit_ref
                != self.state.parent_commit_ref
        {
            return Err(NodeError::Consensus {
                reason: "BFT WAL cannot reconcile without its exact committed pending QC".into(),
            });
        }
        self.persist(state_for_schedule(
            schedule,
            self.local_validator_id.as_str(),
            self.local_signing_public_key.as_str(),
        ))
    }

    fn persist(&mut self, next: BftWalStateV1) -> Result<(), NodeError> {
        if self.protected.is_some() {
            return protected::persist_journal(self, next);
        }
        persist_state(&self.directory, &next)?;
        self.state = next;
        Ok(())
    }
}

impl BftWalStore {
    pub(super) fn open_protected_for_recovery(
        directory: &Path,
        genesis_schedule: &BftHeightProposerScheduleV1,
        binding: &oasis7_distfs::protected_bft::BftSignerBindingV1,
        has_committed_history: bool,
        provider: &dyn oasis7_distfs::protected_bft::BftSignerProviderV1,
        driver_history: &[BftCommitCertificateV1],
    ) -> Result<Self, NodeError> {
        protected::open_for_recovery(
            directory,
            genesis_schedule,
            binding,
            has_committed_history,
            provider,
            driver_history,
        )
    }

    pub(super) fn protected_time_ms(&mut self) -> Result<Option<i64>, NodeError> {
        protected::protected_time_ms(self)
    }

    pub(super) fn sign_reserved_proposal(
        &mut self,
        intent: &BftProposalIntentV1,
    ) -> Result<Option<(BftProposalV1, i64)>, NodeError> {
        protected::sign_reserved_proposal(self, intent)
    }

    pub(super) fn sign_reserved_vote(
        &mut self,
        intent: &BftVoteIntentV1,
    ) -> Result<Option<(BftVoteV1, i64)>, NodeError> {
        protected::sign_reserved_vote(self, intent)
    }
}

fn state_for_schedule(
    schedule: &BftHeightProposerScheduleV1,
    local_validator_id: &str,
    local_signing_public_key: &str,
) -> BftWalStateV1 {
    BftSigningJournalV1::for_schedule(schedule, local_validator_id, local_signing_public_key)
}

fn local_proposal_event<'a>(
    event: &'a BftWalEventV1,
    local_validator_id: &str,
) -> Option<&'a BftProposalV1> {
    match event {
        BftWalEventV1::Proposal {
            proposal, local, ..
        } if *local || proposal.header.proposer_id == local_validator_id => Some(proposal.as_ref()),
        _ => None,
    }
}

fn local_vote_event<'a>(
    event: &'a BftWalEventV1,
    local_validator_id: &str,
) -> Option<&'a BftVoteV1> {
    match event {
        BftWalEventV1::Vote { vote, local, .. }
            if *local || vote.validator_id == local_validator_id =>
        {
            Some(vote.as_ref())
        }
        _ => None,
    }
}

fn proposal_matches_intent(proposal: &BftProposalV1, intent: &BftProposalIntentV1) -> bool {
    let mut header = proposal.header.clone();
    header.signature_hex.clear();
    header == intent.header
        && proposal.block == intent.block
        && proposal.valid_round_evidence == intent.valid_round_evidence
}

fn vote_matches_intent(vote: &BftVoteV1, intent: &BftVoteIntentV1) -> bool {
    vote.height == intent.height
        && vote.round == intent.round
        && vote.phase == intent.phase
        && vote.value == intent.value
        && vote.validator_id == intent.validator_id
}

fn validate_proposal_intent_for_state(
    state: &BftWalStateV1,
    intent: &BftProposalIntentV1,
    local_validator_id: &str,
    local_signing_public_key: &str,
) -> Result<(), NodeError> {
    if intent.header.scope != state.scope
        || intent.header.height != state.height
        || intent.header.round > BFT_MAX_ROUNDS_PER_HEIGHT_V1
        || intent.header.validator_set_hash != state.validator_set_hash
        || intent.header.proposer_id != local_validator_id
        || intent.header.public_key_hex != local_signing_public_key
        || !intent.header.signature_hex.is_empty()
        || intent.block.scope != state.scope
        || intent.block.height != state.height
        || intent.block.parent_commit_ref != state.parent_commit_ref
        || intent.block.authority_epoch != state.authority_epoch
    {
        return Err(NodeError::Consensus {
            reason: "BFT proposal intent differs from the pinned height, scope, or local signer"
                .into(),
        });
    }
    let proposal = BftProposalV1 {
        header: intent.header.clone(),
        block: intent.block.clone(),
        valid_round_evidence: intent.valid_round_evidence.clone(),
    };
    proposal.header.signing_payload().map_err(wal_invalid)?;
    proposal.validate_binding().map_err(wal_invalid)
}

fn validate_vote_intent_for_state(
    state: &BftWalStateV1,
    intent: &BftVoteIntentV1,
    local_validator_id: &str,
) -> Result<(), NodeError> {
    if intent.height != state.height
        || intent.round > BFT_MAX_ROUNDS_PER_HEIGHT_V1
        || intent.validator_id != local_validator_id
    {
        return Err(NodeError::Consensus {
            reason: "BFT vote intent differs from the pinned height or local signer".into(),
        });
    }
    intent.value.validate().map_err(wal_invalid)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BftWalAnchorV1 {
    schema_version: u32,
    scope: BftScopeV1,
    authority_epoch: u64,
    validator_set_hash: String,
    local_validator_id: String,
    local_signing_public_key: String,
}

fn validate_anchor(
    path: &Path,
    schedule: &BftHeightProposerScheduleV1,
    local_validator_id: &str,
    local_signing_public_key: &str,
) -> Result<(), NodeError> {
    let bytes = read_bounded_file(path, "BFT WAL anchor")?;
    let anchor: BftWalAnchorV1 =
        serde_cbor::from_slice(bytes.as_slice()).map_err(|error| NodeError::Consensus {
            reason: format!("decode BFT WAL anchor; preserve evidence and stop: {error}"),
        })?;
    if anchor.schema_version != BFT_WAL_SCHEMA_VERSION
        || anchor.scope != *schedule.scope()
        || anchor.authority_epoch != schedule.authority_epoch()
        || anchor.validator_set_hash != schedule.validator_set_hash()
        || anchor.local_validator_id != local_validator_id
        || anchor.local_signing_public_key != local_signing_public_key
    {
        return Err(NodeError::Consensus {
            reason: "BFT WAL anchor differs from the pinned fixed configuration".into(),
        });
    }
    Ok(())
}

fn persist_anchor(
    directory: &Path,
    schedule: &BftHeightProposerScheduleV1,
    local_validator_id: &str,
    local_signing_public_key: &str,
) -> Result<(), NodeError> {
    let anchor = BftWalAnchorV1 {
        schema_version: BFT_WAL_SCHEMA_VERSION,
        scope: schedule.scope().clone(),
        authority_epoch: schedule.authority_epoch(),
        validator_set_hash: schedule.validator_set_hash().to_string(),
        local_validator_id: local_validator_id.to_string(),
        local_signing_public_key: local_signing_public_key.to_string(),
    };
    let bytes = serde_cbor::to_vec(&anchor).map_err(|error| NodeError::Consensus {
        reason: format!("encode BFT WAL anchor: {error}"),
    })?;
    persist_bytes(directory, BFT_WAL_ANCHOR_FILE, bytes.as_slice())
}

fn validate_schedule_binding(
    state: &BftWalStateV1,
    schedule: &BftHeightProposerScheduleV1,
) -> Result<(), NodeError> {
    if state.schema_version != BFT_WAL_SCHEMA_VERSION
        || state.scope != *schedule.scope()
        || state.authority_epoch != schedule.authority_epoch()
        || state.validator_set_hash != schedule.validator_set_hash()
        || state.height != schedule.height()
        || state.parent_commit_ref != schedule.parent_commit_ref()
        || &state.starting_priority != schedule.starting_priority().priorities()
    {
        return Err(NodeError::Consensus {
            reason: "BFT signer WAL differs from verified committed schedule; stop and preserve evidence".into(),
        });
    }
    Ok(())
}

fn validate_local_identity(
    state: &BftWalStateV1,
    schedule: &BftHeightProposerScheduleV1,
    local_validator_id: &str,
    local_signing_public_key: &str,
) -> Result<(), NodeError> {
    if state.schema_version != BFT_WAL_SCHEMA_VERSION
        || state.scope != *schedule.scope()
        || state.authority_epoch != schedule.authority_epoch()
        || state.validator_set_hash != schedule.validator_set_hash()
        || state.local_validator_id != local_validator_id
        || state.local_signing_public_key != local_signing_public_key
    {
        return Err(NodeError::Consensus {
            reason: "BFT WAL state differs from the pinned local signer configuration".into(),
        });
    }
    Ok(())
}

fn validate_local_wal_events(state: &BftWalStateV1) -> Result<(), NodeError> {
    let mut previous_event_time = None;
    for event in &state.events {
        let now_ms = match event {
            BftWalEventV1::Proposal { now_ms, .. }
            | BftWalEventV1::Vote { now_ms, .. }
            | BftWalEventV1::Timeout { now_ms } => *now_ms,
        };
        let started_at_ms = state
            .height_started_at_ms
            .ok_or_else(|| NodeError::Consensus {
                reason: "BFT WAL has height events without a durable timer origin".into(),
            })?;
        if now_ms < started_at_ms || previous_event_time.is_some_and(|previous| now_ms < previous) {
            return Err(NodeError::Consensus {
                reason:
                    "BFT WAL event times are not monotonic from the durable height timer origin"
                        .into(),
            });
        }
        previous_event_time = Some(now_ms);
        match event {
            BftWalEventV1::Proposal {
                proposal, local, ..
            } => {
                proposal.validate_binding().map_err(wal_invalid)?;
                proposal.header.verify_signature().map_err(wal_invalid)?;
                let claims_local =
                    *local || proposal.header.proposer_id == state.local_validator_id;
                if claims_local
                    && (proposal.header.proposer_id != state.local_validator_id
                        || proposal.header.public_key_hex != state.local_signing_public_key)
                {
                    return Err(NodeError::Consensus {
                        reason: "BFT WAL proposal has a mismatched local signer identity".into(),
                    });
                }
            }
            BftWalEventV1::Vote { vote, local, .. } => {
                vote.verify_signature().map_err(wal_invalid)?;
                let claims_local = *local || vote.validator_id == state.local_validator_id;
                if claims_local
                    && (vote.validator_id != state.local_validator_id
                        || vote.public_key_hex != state.local_signing_public_key)
                {
                    return Err(NodeError::Consensus {
                        reason: "BFT WAL vote has a mismatched local signer identity".into(),
                    });
                }
            }
            BftWalEventV1::Timeout { .. } => {}
        }
    }
    if (state.pending_proposal_intent.is_some()
        || state.pending_vote_intent.is_some()
        || state.pending_commit.is_some())
        && state.height_started_at_ms.is_none()
    {
        return Err(NodeError::Consensus {
            reason: "BFT WAL has a pending decision without a durable height timer origin".into(),
        });
    }
    if let Some(pending_commit) = state.pending_commit.as_ref() {
        pending_commit.validate_wire_bounds().map_err(wal_invalid)?;
    }
    if let Some(intent) = state.pending_proposal_intent.as_ref() {
        validate_proposal_intent_for_state(
            state,
            intent,
            state.local_validator_id.as_str(),
            state.local_signing_public_key.as_str(),
        )?;
    }
    if let Some(intent) = state.pending_vote_intent.as_ref() {
        validate_vote_intent_for_state(state, intent, state.local_validator_id.as_str())?;
    }
    Ok(())
}

fn require_height_start_time(state: &BftWalStateV1) -> Result<(), NodeError> {
    if state.height_started_at_ms.is_none() {
        return Err(NodeError::Consensus {
            reason: "BFT height timer origin must be durable before events or signing".into(),
        });
    }
    Ok(())
}

fn validate_event_time(state: &BftWalStateV1, event: &BftWalEventV1) -> Result<(), NodeError> {
    let started_at_ms = state
        .height_started_at_ms
        .ok_or_else(|| NodeError::Consensus {
            reason: "BFT height timer origin must be durable before events or signing".into(),
        })?;
    let event_time = match event {
        BftWalEventV1::Proposal { now_ms, .. }
        | BftWalEventV1::Vote { now_ms, .. }
        | BftWalEventV1::Timeout { now_ms } => *now_ms,
    };
    let previous_event_time = state.events.last().map(|existing| match existing {
        BftWalEventV1::Proposal { now_ms, .. }
        | BftWalEventV1::Vote { now_ms, .. }
        | BftWalEventV1::Timeout { now_ms } => *now_ms,
    });
    if event_time < started_at_ms
        || previous_event_time.is_some_and(|previous| event_time < previous)
    {
        return Err(NodeError::Consensus {
            reason: "BFT WAL event time precedes its durable timer origin or prior event".into(),
        });
    }
    Ok(())
}

fn wal_invalid(reason: String) -> NodeError {
    NodeError::Consensus {
        reason: format!("invalid signed BFT WAL event; preserve evidence and stop: {reason}"),
    }
}

fn ensure_directory(directory: &Path) -> Result<(), NodeError> {
    if let Ok(metadata) = fs::symlink_metadata(directory)
        && (metadata.file_type().is_symlink() || !metadata.is_dir())
    {
        return Err(NodeError::Consensus {
            reason: "BFT WAL directory must be a real directory, not a symlink or file".into(),
        });
    }
    fs::create_dir_all(directory).map_err(|error| wal_io("create BFT WAL directory", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| wal_io("set BFT WAL directory permissions", error))?;
    }
    Ok(())
}

fn acquire_wal_lock(directory: &Path) -> Result<File, NodeError> {
    let lock_path = directory.join(BFT_WAL_LOCK_FILE);
    if let Ok(metadata) = fs::symlink_metadata(&lock_path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(NodeError::Consensus {
            reason: "BFT WAL lock must be a real regular file".into(),
        });
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| wal_io("open BFT WAL lock", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        lock.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| wal_io("set BFT WAL lock permissions", error))?;
    }
    lock.try_lock().map_err(|error| NodeError::Consensus {
        reason: format!("BFT signer WAL is already in use or cannot be locked: {error}"),
    })?;
    Ok(lock)
}

fn path_exists_without_following(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn read_state(path: &Path) -> Result<BftWalStateV1, NodeError> {
    let bytes = read_bounded_file(path, "BFT WAL")?;
    serde_cbor::from_slice(bytes.as_slice()).map_err(|error| NodeError::Consensus {
        reason: format!("decode BFT signer WAL; preserve evidence and stop: {error}"),
    })
}

fn read_bounded_file(path: &Path, label: &str) -> Result<Vec<u8>, NodeError> {
    read_bounded_file_with_limit(path, label, BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1)
}

fn read_bounded_file_with_limit(
    path: &Path,
    label: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, NodeError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| wal_io("stat BFT WAL", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(NodeError::Consensus {
            reason: format!("{label} must be a real regular file"),
        });
    }
    if usize::try_from(metadata.len()).unwrap_or(usize::MAX) > max_bytes {
        return Err(NodeError::Consensus {
            reason: format!("{label} exceeds its bounded storage capacity"),
        });
    }
    let file = File::open(path).map_err(|error| wal_io("open BFT WAL", error))?;
    let initial_capacity = usize::try_from(metadata.len())
        .unwrap_or(max_bytes)
        .min(max_bytes);
    read_bounded_reader(file, label, max_bytes, initial_capacity)
}

fn read_bounded_reader(
    reader: impl Read,
    label: &str,
    max_bytes: usize,
    initial_capacity: usize,
) -> Result<Vec<u8>, NodeError> {
    let read_limit = u64::try_from(max_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::with_capacity(initial_capacity);
    let mut bounded = reader.take(read_limit);
    bounded
        .read_to_end(&mut bytes)
        .map_err(|error| wal_io("read BFT WAL", error))?;
    if bytes.len() > max_bytes {
        return Err(NodeError::Consensus {
            reason: format!("{label} exceeds its bounded storage capacity"),
        });
    }
    Ok(bytes)
}

fn persist_state(directory: &Path, state: &BftWalStateV1) -> Result<(), NodeError> {
    let bytes = serde_cbor::to_vec(state).map_err(|error| NodeError::Consensus {
        reason: format!("encode BFT signer WAL: {error}"),
    })?;
    if bytes.len() > BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1 {
        return Err(NodeError::Consensus {
            reason: "BFT signer WAL capacity exhausted; preserve lock and stop signing".into(),
        });
    }
    persist_bytes(directory, BFT_WAL_FILE, bytes.as_slice())
}

fn persist_bytes(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), NodeError> {
    let state_path = directory.join(name);
    let temp_path = directory.join(format!("{name}.tmp-{}", std::process::id()));
    if temp_path.exists() {
        let metadata = fs::symlink_metadata(&temp_path)
            .map_err(|error| wal_io("stat stale BFT WAL temporary file", error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(NodeError::Consensus {
                reason: "BFT WAL temporary path is not a regular file; preserve evidence and stop"
                    .into(),
            });
        }
        fs::remove_file(&temp_path)
            .map_err(|error| wal_io("remove interrupted BFT WAL temporary file", error))?;
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)
        .map_err(|error| wal_io("create BFT WAL temporary file", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| wal_io("set BFT WAL permissions", error))?;
    }
    file.write_all(bytes)
        .map_err(|error| wal_io("write BFT WAL", error))?;
    file.sync_all()
        .map_err(|error| wal_io("sync BFT WAL", error))?;
    drop(file);
    fs::rename(&temp_path, &state_path).map_err(|error| wal_io("replace BFT WAL", error))?;
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| wal_io("sync BFT WAL directory", error))
}

fn wal_io(operation: &str, error: std::io::Error) -> NodeError {
    NodeError::Consensus {
        reason: format!("{operation} failed; preserve BFT signer state and stop: {error}"),
    }
}

#[cfg(test)]
#[path = "node_bft_wal_tests.rs"]
mod tests;

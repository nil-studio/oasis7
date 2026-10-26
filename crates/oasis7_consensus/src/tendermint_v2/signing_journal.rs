use std::collections::HashSet;
use std::io::{self, Write};

use oasis7_proto::distributed_bft::v2::{
    BftCommitCertificateV2, BftProposalHeaderV2, BftProposalV2, BftScopeV2, BftVoteV2,
};
use serde::{Deserialize, Serialize};

use super::{
    BFT_MAX_ROUNDS_PER_HEIGHT_V2, BftConsensusActionV2, BftConsensusMachineV2,
    BftHeightProposerScheduleV2, BftProposalIntentV2, BftValidatorSetV2, BftVoteIntentV2,
    verify_commit_certificate_token_v2,
};
use crate::tendermint_v1::BftTimeoutsV1;

pub const BFT_MAX_SIGNING_JOURNAL_BYTES_V2: usize = 256 * 1024 * 1024;
const BFT_SIGNING_JOURNAL_DIGEST_DOMAIN_V2: &[u8] = b"oasis7-bft-signing-journal-v2|";
const BFT_SIGNING_JOURNAL_EVENT_DOMAIN_V2: &[u8] = b"oasis7-bft-signing-journal-event-v2|";
const BFT_MAX_OUTSTANDING_SIGN_INTENTS_V2: usize = 2 * (BFT_MAX_ROUNDS_PER_HEIGHT_V2 as usize + 1);

/// The durable, single-height signer journal shared by the local WAL and a
/// protected signer. V2 is a distinct schema and signing domain; V1 journals
/// cannot be decoded or replayed as V2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSigningJournalV2 {
    pub schema_version: u32,
    pub scope: BftScopeV2,
    pub authority_epoch: u64,
    pub active_validator_set_hash: String,
    pub schedule_policy_hash: String,
    pub schedule_digest: String,
    pub local_validator_id: String,
    pub local_signing_public_key: String,
    pub height: u64,
    pub parent_commit_ref: String,
    pub parent_tick: u64,
    pub parent_state_root: String,
    pub parent_manifest_hash: String,
    #[serde(with = "super::priority_wire")]
    pub starting_priority: std::collections::BTreeMap<String, i128>,
    pub height_started_at_ms: Option<i64>,
    pub events: Vec<BftSigningJournalEventV2>,
    pub pending_proposal_intent: Option<BftProposalIntentV2>,
    pub pending_vote_intent: Option<BftVoteIntentV2>,
    pub pending_commit: Option<BftCommitCertificateV2>,
}

/// V2 event enum with an independent schema and V2 signed material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum BftSigningJournalEventV2 {
    Proposal {
        proposal: Box<BftProposalV2>,
        local: bool,
        now_ms: i64,
    },
    ProposalEquivocation {
        first: Box<BftProposalHeaderV2>,
        second: Box<BftProposalHeaderV2>,
        now_ms: i64,
    },
    Vote {
        vote: Box<BftVoteV2>,
        local: bool,
        now_ms: i64,
    },
    Timeout {
        now_ms: i64,
    },
}

impl BftSigningJournalEventV2 {
    pub fn event_time_ms(&self) -> i64 {
        match self {
            Self::Proposal { now_ms, .. }
            | Self::ProposalEquivocation { now_ms, .. }
            | Self::Vote { now_ms, .. }
            | Self::Timeout { now_ms } => *now_ms,
        }
    }

    pub fn is_local(&self) -> bool {
        match self {
            Self::Proposal { local, .. } | Self::Vote { local, .. } => *local,
            Self::ProposalEquivocation { .. } | Self::Timeout { .. } => false,
        }
    }
}

impl BftSigningJournalV2 {
    /// Construct an empty journal for the exact schedule already authorized
    /// by the caller. This does not establish or persist trust in that schedule.
    pub fn for_schedule(
        schedule: &BftHeightProposerScheduleV2,
        local_validator_id: impl Into<String>,
        local_signing_public_key: impl Into<String>,
    ) -> Result<Self, String> {
        let local_validator_id = local_validator_id.into();
        let local_signing_public_key = local_signing_public_key.into();
        validate_local_identity(schedule, &local_validator_id, &local_signing_public_key)?;
        Ok(Self {
            schema_version: 2,
            scope: schedule.scope().clone(),
            authority_epoch: schedule.authority_epoch(),
            active_validator_set_hash: schedule.active_set().set_hash().to_string(),
            schedule_policy_hash: schedule.policy_hash().to_string(),
            schedule_digest: schedule.digest()?,
            local_validator_id,
            local_signing_public_key,
            height: schedule.height(),
            parent_commit_ref: schedule.parent_commit_ref().to_string(),
            parent_tick: schedule.parent_tick(),
            parent_state_root: schedule.parent_state_root().to_string(),
            parent_manifest_hash: schedule.parent_manifest_hash().to_string(),
            starting_priority: schedule.starting_priority().priorities().clone(),
            height_started_at_ms: None,
            events: Vec::new(),
            pending_proposal_intent: None,
            pending_vote_intent: None,
            pending_commit: None,
        })
    }

    pub fn events(&self) -> &[BftSigningJournalEventV2] {
        &self.events
    }

    pub fn height(&self) -> u64 {
        self.height
    }

    pub fn scope(&self) -> &BftScopeV2 {
        &self.scope
    }

    pub fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }

    pub fn active_validator_set_hash(&self) -> &str {
        &self.active_validator_set_hash
    }

    pub fn schedule_policy_hash(&self) -> &str {
        &self.schedule_policy_hash
    }

    pub fn schedule_digest(&self) -> &str {
        &self.schedule_digest
    }

    pub fn local_validator_id(&self) -> &str {
        &self.local_validator_id
    }

    pub fn local_signing_public_key(&self) -> &str {
        &self.local_signing_public_key
    }

    pub fn parent_commit_ref(&self) -> &str {
        &self.parent_commit_ref
    }

    pub fn parent_tick(&self) -> u64 {
        self.parent_tick
    }

    pub fn parent_state_root(&self) -> &str {
        &self.parent_state_root
    }

    pub fn parent_manifest_hash(&self) -> &str {
        &self.parent_manifest_hash
    }

    pub fn pending_proposal_intent(&self) -> Option<&BftProposalIntentV2> {
        self.pending_proposal_intent.as_ref()
    }

    pub fn pending_vote_intent(&self) -> Option<&BftVoteIntentV2> {
        self.pending_vote_intent.as_ref()
    }

    pub fn pending_commit(&self) -> Option<&BftCommitCertificateV2> {
        self.pending_commit.as_ref()
    }

    pub fn height_started_at_ms(&self) -> Option<i64> {
        self.height_started_at_ms
    }

    /// Hash the canonical bounded CBOR representation. This is an identity
    /// digest, not an authorization proof or rollback-resistant checkpoint.
    pub fn digest(&self) -> Result<String, String> {
        let (digest, _) = hash_bounded(self, BFT_SIGNING_JOURNAL_DIGEST_DOMAIN_V2)?;
        Ok(digest.to_hex().to_string())
    }

    pub fn encoded_len(&self) -> Result<usize, String> {
        hash_bounded(self, &[]).map(|(_, encoded_len)| encoded_len)
    }
}

/// Replay a complete same-height journal from its externally authorized
/// height schedule. The returned machine is protocol state only; callers must
/// still run application execution validation for proposals.
pub fn replay_bft_signing_journal_v2(
    journal: &BftSigningJournalV2,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
    timeouts: BftTimeoutsV1,
) -> Result<BftConsensusMachineV2, String> {
    replay_journal(journal, set, schedule, timeouts).map(|replay| replay.machine)
}

/// Verify that an existing journal durably reserved this exact certificate
/// and derive its successor schedule without constructing a journal for the
/// previous signer identity. This is required when the committed transition
/// removes the signer or rotates its key; it does not authorize signing under
/// the successor set.
pub fn verify_reserved_bft_commit_transition_v2(
    journal: &BftSigningJournalV2,
    certificate: &BftCommitCertificateV2,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
    timeouts: BftTimeoutsV1,
) -> Result<BftHeightProposerScheduleV2, String> {
    replay_bft_signing_journal_v2(journal, set, schedule, timeouts)?;
    if journal.pending_commit() != Some(certificate) {
        return Err("BFT successor requires the exact durably reserved commit".into());
    }
    let token = verify_commit_certificate_token_v2(schedule, certificate)?;
    Ok(token.next_height_schedule().clone())
}

struct JournalReplay {
    machine: BftConsensusMachineV2,
    outstanding_vote_intents: Vec<BftVoteIntentV2>,
}

fn replay_journal(
    journal: &BftSigningJournalV2,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
    timeouts: BftTimeoutsV1,
) -> Result<JournalReplay, String> {
    validate_static_binding(journal, set, schedule)?;
    timeouts.validate()?;
    hash_bounded(journal, &[])?;
    let start_ms = journal.height_started_at_ms.unwrap_or(0);
    if start_ms < 0 {
        return Err("BFT signing journal timer origin cannot be negative".into());
    }
    if journal.height_started_at_ms.is_none()
        && (!journal.events.is_empty()
            || journal.pending_proposal_intent.is_some()
            || journal.pending_vote_intent.is_some()
            || journal.pending_commit.is_some())
    {
        return Err("BFT signing journal has height state without a durable timer origin".into());
    }
    let pending_count = usize::from(journal.pending_proposal_intent.is_some())
        + usize::from(journal.pending_vote_intent.is_some())
        + usize::from(journal.pending_commit.is_some());
    if pending_count > 1 {
        return Err("BFT signing journal contains conflicting pending decisions".into());
    }

    let mut machine = BftConsensusMachineV2::new(
        schedule.clone(),
        journal.local_validator_id.clone(),
        start_ms,
        timeouts,
    )?;
    let mut previous_time = journal.height_started_at_ms;
    let mut emitted_vote_intents = Vec::<BftVoteIntentV2>::new();
    let mut seen_events = HashSet::<[u8; 32]>::new();

    for (index, event) in journal.events.iter().enumerate() {
        let now_ms = event.event_time_ms();
        if now_ms < 0 || previous_time.is_some_and(|previous| now_ms < previous) {
            return Err(format!(
                "BFT signing journal event {index} has a non-monotonic timestamp"
            ));
        }
        previous_time = Some(now_ms);
        let (event_digest, _) = hash_bounded(event, BFT_SIGNING_JOURNAL_EVENT_DOMAIN_V2)?;
        if !seen_events.insert(*event_digest.as_bytes()) {
            return Err(
                "BFT signing journal repeats an event instead of recording one transition".into(),
            );
        }

        let actions = match event {
            BftSigningJournalEventV2::Proposal {
                proposal,
                local,
                now_ms,
            } => {
                if *local != (proposal.header.proposer_id == journal.local_validator_id) {
                    return Err("BFT proposal event local marker differs from its signer".into());
                }
                if *local {
                    let expected = machine.proposal_intent(
                        proposal.block.clone(),
                        journal.local_validator_id.as_str(),
                    )?;
                    if !proposal_matches_intent(proposal, &expected) {
                        return Err(
                            "local BFT proposal differs from the replayed signing decision".into(),
                        );
                    }
                }
                machine.receive_proposal(proposal.as_ref().clone(), true, *now_ms)?
            }
            BftSigningJournalEventV2::ProposalEquivocation { first, second, .. } => {
                machine.record_proposal_equivocation_evidence(first, second)?;
                Vec::new()
            }
            BftSigningJournalEventV2::Vote {
                vote,
                local,
                now_ms,
            } => {
                if *local != (vote.validator_id == journal.local_validator_id) {
                    return Err("BFT vote event local marker differs from its signer".into());
                }
                if *local {
                    let expected = vote_intent_from_vote(vote.as_ref());
                    if !emitted_vote_intents.contains(&expected) {
                        return Err(
                            "local BFT vote has no matching deterministic signing request".into(),
                        );
                    }
                    emitted_vote_intents.retain(|intent| intent != &expected);
                    machine.record_local_vote(vote.as_ref().clone(), *now_ms)?
                } else {
                    machine.receive_vote(vote.as_ref().clone(), *now_ms)?
                }
            }
            BftSigningJournalEventV2::Timeout { now_ms } => {
                let old_round = machine.round();
                let old_step = machine.step();
                if machine
                    .step_deadline_ms()
                    .is_none_or(|deadline| *now_ms < deadline)
                {
                    return Err("BFT timeout journal event precedes the replayed deadline".into());
                }
                let actions = machine.on_timeout(*now_ms, journal.local_validator_id.as_str())?;
                if machine.round() == old_round && machine.step() == old_step {
                    return Err(
                        "BFT timeout journal event does not advance the protocol state".into(),
                    );
                }
                actions
            }
        };
        for action in actions {
            match action {
                BftConsensusActionV2::SignVote(intent) => {
                    if !emitted_vote_intents.contains(&intent) {
                        if emitted_vote_intents.len() >= BFT_MAX_OUTSTANDING_SIGN_INTENTS_V2 {
                            return Err(
                                "BFT signing journal has too many unresolved vote intents".into()
                            );
                        }
                        emitted_vote_intents.push(intent);
                    }
                }
                BftConsensusActionV2::Commit(_) => {}
                BftConsensusActionV2::RoundAdvanced { .. } => {}
            }
        }
    }

    if let Some(intent) = journal.pending_proposal_intent.as_ref() {
        let expected =
            machine.proposal_intent(intent.block.clone(), journal.local_validator_id.as_str())?;
        if &expected != intent {
            return Err("pending BFT proposal intent differs from deterministic replay".into());
        }
    }
    if let Some(intent) = journal.pending_vote_intent.as_ref() {
        let expected = machine
            .vote_intent(
                intent.validator_id.clone(),
                intent.phase,
                intent.value.clone(),
                intent.round,
            )?
            .ok_or_else(|| "pending BFT vote intent is already completed".to_string())?;
        if &expected != intent || !emitted_vote_intents.contains(intent) {
            return Err("pending BFT vote intent differs from deterministic replay".into());
        }
    }
    if let Some(certificate) = journal.pending_commit.as_ref() {
        verify_commit_certificate_token_v2(schedule, certificate)?;
    }
    Ok(JournalReplay {
        machine,
        outstanding_vote_intents: emitted_vote_intents,
    })
}

/// Verify one legal durable journal transition. The only height advance is
/// exact completion of a previously reserved and independently verified QC.
pub fn verify_bft_signing_journal_transition_v2(
    old: &BftSigningJournalV2,
    next: &BftSigningJournalV2,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
    timeouts: BftTimeoutsV1,
) -> Result<BftHeightProposerScheduleV2, String> {
    let machine = replay_bft_signing_journal_v2(old, set, schedule, timeouts)?;
    hash_bounded(next, &[])?;
    if old == next {
        return Ok(schedule.clone());
    }

    if let Some(certificate) = old.pending_commit.as_ref() {
        let token = verify_commit_certificate_token_v2(schedule, certificate)?;
        let expected = BftSigningJournalV2::for_schedule(
            token.next_height_schedule(),
            old.local_validator_id.clone(),
            old.local_signing_public_key.clone(),
        )?;
        if &expected == next {
            return Ok(token.next_height_schedule().clone());
        }
    }

    let mut expected = old.clone();
    if expected.height_started_at_ms.is_none()
        && let Some(now_ms) = next.height_started_at_ms
    {
        if now_ms < 0 {
            return Err("BFT height timer origin cannot be negative".into());
        }
        expected.height_started_at_ms = Some(now_ms);
        if &expected == next {
            return Ok(schedule.clone());
        }
    }

    if expected.pending_proposal_intent.is_none()
        && let Some(intent) = next.pending_proposal_intent.as_ref()
    {
        if old.height_started_at_ms.is_none()
            || old.pending_vote_intent.is_some()
            || old.pending_commit.is_some()
        {
            return Err(
                "proposal reservation requires a started height and an empty signer slot".into(),
            );
        }
        expected.pending_proposal_intent = Some(intent.clone());
        if &expected == next {
            let exact =
                machine.proposal_intent(intent.block.clone(), old.local_validator_id.as_str())?;
            if &exact != intent {
                return Err("proposal reservation is not the exact deterministic decision".into());
            }
            return Ok(schedule.clone());
        }
    }

    if expected.pending_vote_intent.is_none()
        && let Some(intent) = next.pending_vote_intent.as_ref()
    {
        if old.height_started_at_ms.is_none()
            || old.pending_proposal_intent.is_some()
            || old.pending_commit.is_some()
        {
            return Err(
                "vote reservation requires a started height and an empty signer slot".into(),
            );
        }
        expected.pending_vote_intent = Some(intent.clone());
        if &expected == next {
            let exact = machine
                .vote_intent(
                    intent.validator_id.clone(),
                    intent.phase,
                    intent.value.clone(),
                    intent.round,
                )?
                .ok_or_else(|| "vote reservation is already completed".to_string())?;
            let replay = replay_journal(old, set, schedule, timeouts)?;
            if &exact != intent || !replay.outstanding_vote_intents.contains(intent) {
                return Err(
                    "vote reservation is not a replayed deterministic signing request".into(),
                );
            }
            return Ok(schedule.clone());
        }
        expected = old.clone();
    }

    if expected.pending_commit.is_none()
        && let Some(certificate) = next.pending_commit.as_ref()
    {
        if old.height_started_at_ms.is_none()
            || old.pending_proposal_intent.is_some()
            || old.pending_vote_intent.is_some()
        {
            return Err(
                "commit reservation requires a started height and an empty signer slot".into(),
            );
        }
        expected.pending_commit = Some(certificate.clone());
        if &expected == next {
            let token = verify_commit_certificate_token_v2(schedule, certificate)?;
            if token.schedule() != schedule {
                return Err("commit reservation schedule changed".into());
            }
            return Ok(schedule.clone());
        }
    }

    if next.events.len() == old.events.len() + 1 && next.events.starts_with(old.events.as_slice()) {
        let event = next.events.last().expect("length checked");
        expected = old.clone();
        expected.events.push(event.clone());
        match event {
            BftSigningJournalEventV2::Proposal {
                proposal,
                local: true,
                ..
            } => {
                let intent = machine
                    .proposal_intent(proposal.block.clone(), old.local_validator_id.as_str())?;
                if old.pending_proposal_intent.as_ref() != Some(&intent)
                    || old.pending_vote_intent.is_some()
                    || old.pending_commit.is_some()
                    || !proposal_matches_intent(proposal, &intent)
                {
                    return Err(
                        "local proposal event does not complete its exact pending intent".into(),
                    );
                }
                expected.pending_proposal_intent = None;
            }
            BftSigningJournalEventV2::Vote {
                vote, local: true, ..
            } => {
                let intent = vote_intent_from_vote(vote.as_ref());
                if old.pending_vote_intent.as_ref() != Some(&intent)
                    || old.pending_proposal_intent.is_some()
                    || old.pending_commit.is_some()
                {
                    return Err(
                        "local vote event does not complete its exact pending intent".into(),
                    );
                }
                expected.pending_vote_intent = None;
            }
            BftSigningJournalEventV2::ProposalEquivocation { .. } => {}
            BftSigningJournalEventV2::Proposal { local: false, .. }
            | BftSigningJournalEventV2::Vote { local: false, .. }
            | BftSigningJournalEventV2::Timeout { .. } => {
                if old.pending_proposal_intent.is_some()
                    || old.pending_vote_intent.is_some()
                    || old.pending_commit.is_some()
                {
                    return Err(
                        "external BFT event cannot overtake a pending signer decision".into(),
                    );
                }
            }
        }
        if &expected == next {
            replay_bft_signing_journal_v2(next, set, schedule, timeouts)?;
            return Ok(schedule.clone());
        }
    }

    Err("BFT signing journal change is not one permitted durable transition".into())
}

pub fn complete_reserved_bft_proposal_v2(
    current: &BftSigningJournalV2,
    proposal: BftProposalV2,
    now_ms: i64,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
    timeouts: BftTimeoutsV1,
) -> Result<BftSigningJournalV2, String> {
    replay_bft_signing_journal_v2(current, set, schedule, timeouts)?;
    let intent = current
        .pending_proposal_intent
        .as_ref()
        .ok_or_else(|| "no BFT proposal signing intent is pending".to_string())?;
    if !proposal_matches_intent(&proposal, intent) {
        return Err("signed BFT proposal does not match its pending intent".into());
    }
    let mut next = current.clone();
    next.events.push(BftSigningJournalEventV2::Proposal {
        proposal: Box::new(proposal),
        local: true,
        now_ms,
    });
    next.pending_proposal_intent = None;
    verify_bft_signing_journal_transition_v2(current, &next, set, schedule, timeouts)?;
    Ok(next)
}

pub fn complete_reserved_bft_vote_v2(
    current: &BftSigningJournalV2,
    vote: BftVoteV2,
    now_ms: i64,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
    timeouts: BftTimeoutsV1,
) -> Result<BftSigningJournalV2, String> {
    replay_bft_signing_journal_v2(current, set, schedule, timeouts)?;
    let intent = current
        .pending_vote_intent
        .as_ref()
        .ok_or_else(|| "no BFT vote signing intent is pending".to_string())?;
    if vote_intent_from_vote(&vote) != *intent {
        return Err("signed BFT vote does not match its pending intent".into());
    }
    let mut next = current.clone();
    next.events.push(BftSigningJournalEventV2::Vote {
        vote: Box::new(vote),
        local: true,
        now_ms,
    });
    next.pending_vote_intent = None;
    verify_bft_signing_journal_transition_v2(current, &next, set, schedule, timeouts)?;
    Ok(next)
}

fn validate_static_binding(
    journal: &BftSigningJournalV2,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
) -> Result<(), String> {
    let expected_schedule_digest = schedule.digest()?;
    validate_local_identity(
        schedule,
        &journal.local_validator_id,
        &journal.local_signing_public_key,
    )?;
    if journal.schema_version != 2
        || journal.scope != *schedule.scope()
        || journal.scope != *set.scope()
        || journal.authority_epoch != schedule.authority_epoch()
        || journal.active_validator_set_hash != schedule.active_set().set_hash()
        || journal.active_validator_set_hash != set.set_hash()
        || journal.schedule_policy_hash != schedule.policy_hash()
        || journal.schedule_digest != expected_schedule_digest
        || journal.height != schedule.height()
        || journal.parent_commit_ref != schedule.parent_commit_ref()
        || journal.parent_tick != schedule.parent_tick()
        || journal.parent_state_root != schedule.parent_state_root()
        || journal.parent_manifest_hash != schedule.parent_manifest_hash()
        || journal.starting_priority != *schedule.starting_priority().priorities()
    {
        return Err("BFT signing journal differs from its authorized schedule or signer".into());
    }
    Ok(())
}

fn validate_local_identity(
    schedule: &BftHeightProposerScheduleV2,
    local_validator_id: &str,
    local_signing_public_key: &str,
) -> Result<(), String> {
    if local_validator_id.trim().is_empty()
        || local_validator_id.len() > 128
        || local_signing_public_key.len() != 64
        || !local_signing_public_key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("BFT V2 journal local identity is invalid".into());
    }
    let key_bytes: [u8; 32] = hex::decode(local_signing_public_key)
        .map_err(|_| "BFT V2 journal local public key is invalid")?
        .try_into()
        .map_err(|_| "BFT V2 journal local public key is invalid")?;
    ed25519_dalek::VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| "BFT V2 journal local public key is invalid")?;
    if schedule
        .active_set()
        .validator(local_validator_id)
        .is_some_and(|validator| validator.public_key_hex != local_signing_public_key)
    {
        return Err("BFT V2 journal active local key differs from validator set".into());
    }
    Ok(())
}

fn vote_intent_from_vote(vote: &BftVoteV2) -> BftVoteIntentV2 {
    BftVoteIntentV2 {
        height: vote.height,
        round: vote.round,
        phase: vote.phase,
        value: vote.value.clone(),
        validator_id: vote.validator_id.clone(),
        schedule_policy_hash: vote.schedule_policy_hash.clone(),
    }
}

fn proposal_matches_intent(proposal: &BftProposalV2, intent: &BftProposalIntentV2) -> bool {
    let mut header = proposal.header.clone();
    header.signature_hex.clear();
    header == intent.header
        && proposal.block == intent.block
        && proposal.valid_round_evidence == intent.valid_round_evidence
}

fn hash_bounded<T: Serialize>(value: &T, domain: &[u8]) -> Result<(blake3::Hash, usize), String> {
    let mut writer = BoundedHasher::new(BFT_MAX_SIGNING_JOURNAL_BYTES_V2);
    writer.hasher.update(domain);
    let mut serializer =
        serde_cbor::ser::Serializer::new(serde_cbor::ser::IoWrite::new(&mut writer));
    value
        .serialize(&mut serializer)
        .map_err(|error| format!("hash bounded BFT signing journal: {error}"))?;
    Ok((writer.hasher.finalize(), writer.bytes_written))
}

struct BoundedHasher {
    hasher: blake3::Hasher,
    bytes_written: usize,
    limit: usize,
}

impl BoundedHasher {
    fn new(limit: usize) -> Self {
        Self {
            hasher: blake3::Hasher::new(),
            bytes_written: 0,
            limit,
        }
    }
}

impl Write for BoundedHasher {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let next_len = self
            .bytes_written
            .checked_add(buffer.len())
            .ok_or_else(|| io::Error::other("BFT signing journal size overflow"))?;
        if next_len > self.limit {
            return Err(io::Error::other(
                "BFT signing journal exceeds its byte bound",
            ));
        }
        self.hasher.update(buffer);
        self.bytes_written = next_len;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "signing_journal_tests.rs"]
mod tests;

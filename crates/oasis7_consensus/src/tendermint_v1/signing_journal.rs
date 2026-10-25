use std::collections::HashSet;
use std::io::{self, Write};

use oasis7_proto::distributed_bft::{
    BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1, BftCommitCertificateV1, BftProposalV1, BftScopeV1,
    BftVoteV1,
};
use serde::{Deserialize, Serialize};

use super::{
    BFT_MAX_ROUNDS_PER_HEIGHT_V1, BftConsensusActionV1, BftConsensusMachineV1,
    BftHeightProposerScheduleV1, BftProposalIntentV1, BftTimeoutsV1, BftValidatorSetV1,
    BftVoteIntentV1, verify_commit_certificate_token_v1,
};

pub const BFT_MAX_SIGNING_JOURNAL_BYTES_V1: usize = BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1;
const BFT_SIGNING_JOURNAL_DIGEST_DOMAIN_V1: &[u8] = b"oasis7-bft-signing-journal-v1|";
const BFT_SIGNING_JOURNAL_EVENT_DOMAIN_V1: &[u8] = b"oasis7-bft-signing-journal-event-v1|";
const BFT_MAX_OUTSTANDING_SIGN_INTENTS_V1: usize = 2 * (BFT_MAX_ROUNDS_PER_HEIGHT_V1 as usize + 1);

/// The durable, single-height signer journal shared by the local WAL and a
/// protected signer. The schema and field order intentionally match the
/// existing Node schema-4 WAL so local persistence stays byte compatible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSigningJournalV1 {
    pub schema_version: u32,
    pub scope: BftScopeV1,
    pub authority_epoch: u64,
    pub validator_set_hash: String,
    pub local_validator_id: String,
    pub local_signing_public_key: String,
    pub height: u64,
    pub parent_commit_ref: String,
    pub starting_priority: std::collections::BTreeMap<String, i128>,
    pub height_started_at_ms: Option<i64>,
    pub events: Vec<BftSigningJournalEventV1>,
    pub pending_proposal_intent: Option<BftProposalIntentV1>,
    pub pending_vote_intent: Option<BftVoteIntentV1>,
    pub pending_commit: Option<BftCommitCertificateV1>,
}

/// Events are byte-compatible with the previous Node-local schema-4 enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum BftSigningJournalEventV1 {
    Proposal {
        proposal: Box<BftProposalV1>,
        local: bool,
        now_ms: i64,
    },
    Vote {
        vote: Box<BftVoteV1>,
        local: bool,
        now_ms: i64,
    },
    Timeout {
        now_ms: i64,
    },
}

impl BftSigningJournalEventV1 {
    pub fn event_time_ms(&self) -> i64 {
        match self {
            Self::Proposal { now_ms, .. }
            | Self::Vote { now_ms, .. }
            | Self::Timeout { now_ms } => *now_ms,
        }
    }

    pub fn is_local(&self) -> bool {
        match self {
            Self::Proposal { local, .. } | Self::Vote { local, .. } => *local,
            Self::Timeout { .. } => false,
        }
    }
}

impl BftSigningJournalV1 {
    /// Construct an empty journal for the exact schedule already authorized
    /// by the caller. This does not establish or persist trust in that schedule.
    pub fn for_schedule(
        schedule: &BftHeightProposerScheduleV1,
        local_validator_id: impl Into<String>,
        local_signing_public_key: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: 4,
            scope: schedule.scope().clone(),
            authority_epoch: schedule.authority_epoch(),
            validator_set_hash: schedule.validator_set_hash().to_string(),
            local_validator_id: local_validator_id.into(),
            local_signing_public_key: local_signing_public_key.into(),
            height: schedule.height(),
            parent_commit_ref: schedule.parent_commit_ref().to_string(),
            starting_priority: schedule.starting_priority().priorities().clone(),
            height_started_at_ms: None,
            events: Vec::new(),
            pending_proposal_intent: None,
            pending_vote_intent: None,
            pending_commit: None,
        }
    }

    pub fn events(&self) -> &[BftSigningJournalEventV1] {
        &self.events
    }

    pub fn pending_proposal_intent(&self) -> Option<&BftProposalIntentV1> {
        self.pending_proposal_intent.as_ref()
    }

    pub fn pending_vote_intent(&self) -> Option<&BftVoteIntentV1> {
        self.pending_vote_intent.as_ref()
    }

    pub fn pending_commit(&self) -> Option<&BftCommitCertificateV1> {
        self.pending_commit.as_ref()
    }

    pub fn height_started_at_ms(&self) -> Option<i64> {
        self.height_started_at_ms
    }

    /// Hash the canonical bounded CBOR representation. This is an identity
    /// digest, not an authorization proof or rollback-resistant checkpoint.
    pub fn digest(&self) -> Result<String, String> {
        let (digest, _) = hash_bounded(self, BFT_SIGNING_JOURNAL_DIGEST_DOMAIN_V1)?;
        Ok(digest.to_hex().to_string())
    }

    pub fn encoded_len(&self) -> Result<usize, String> {
        hash_bounded(self, &[]).map(|(_, encoded_len)| encoded_len)
    }
}

/// Replay a complete same-height journal from its externally authorized
/// height schedule. The returned machine is protocol state only; callers must
/// still run application execution validation for proposals.
pub fn replay_bft_signing_journal_v1(
    journal: &BftSigningJournalV1,
    set: &BftValidatorSetV1,
    schedule: &BftHeightProposerScheduleV1,
    timeouts: BftTimeoutsV1,
) -> Result<BftConsensusMachineV1, String> {
    replay_journal(journal, set, schedule, timeouts).map(|replay| replay.machine)
}

struct JournalReplay {
    machine: BftConsensusMachineV1,
    outstanding_vote_intents: Vec<BftVoteIntentV1>,
}

fn replay_journal(
    journal: &BftSigningJournalV1,
    set: &BftValidatorSetV1,
    schedule: &BftHeightProposerScheduleV1,
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

    let mut machine = BftConsensusMachineV1::new(
        set.clone(),
        journal.local_validator_id.clone(),
        journal.height,
        Some(schedule.starting_priority().clone()),
        start_ms,
        timeouts,
    )?;
    let mut previous_time = journal.height_started_at_ms;
    let mut emitted_vote_intents = Vec::<BftVoteIntentV1>::new();
    let mut seen_events = HashSet::<[u8; 32]>::new();

    for (index, event) in journal.events.iter().enumerate() {
        let now_ms = event.event_time_ms();
        if now_ms < 0 || previous_time.is_some_and(|previous| now_ms < previous) {
            return Err(format!(
                "BFT signing journal event {index} has a non-monotonic timestamp"
            ));
        }
        previous_time = Some(now_ms);
        let (event_digest, _) = hash_bounded(event, BFT_SIGNING_JOURNAL_EVENT_DOMAIN_V1)?;
        if !seen_events.insert(*event_digest.as_bytes()) {
            return Err(
                "BFT signing journal repeats an event instead of recording one transition".into(),
            );
        }

        let actions = match event {
            BftSigningJournalEventV1::Proposal {
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
            BftSigningJournalEventV1::Vote {
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
            BftSigningJournalEventV1::Timeout { now_ms } => {
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
                BftConsensusActionV1::SignVote(intent) => {
                    if !emitted_vote_intents.contains(&intent) {
                        if emitted_vote_intents.len() >= BFT_MAX_OUTSTANDING_SIGN_INTENTS_V1 {
                            return Err(
                                "BFT signing journal has too many unresolved vote intents".into()
                            );
                        }
                        emitted_vote_intents.push(intent);
                    }
                }
                BftConsensusActionV1::Commit(_) => {}
                BftConsensusActionV1::RoundAdvanced { .. } => {}
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
        verify_commit_certificate_token_v1(set, schedule, certificate)?;
    }
    Ok(JournalReplay {
        machine,
        outstanding_vote_intents: emitted_vote_intents,
    })
}

/// Verify one legal durable journal transition. The only height advance is
/// exact completion of a previously reserved and independently verified QC.
pub fn verify_bft_signing_journal_transition_v1(
    old: &BftSigningJournalV1,
    next: &BftSigningJournalV1,
    set: &BftValidatorSetV1,
    schedule: &BftHeightProposerScheduleV1,
    timeouts: BftTimeoutsV1,
) -> Result<BftHeightProposerScheduleV1, String> {
    let machine = replay_bft_signing_journal_v1(old, set, schedule, timeouts)?;
    hash_bounded(next, &[])?;
    if old == next {
        return Ok(schedule.clone());
    }

    if let Some(certificate) = old.pending_commit.as_ref() {
        let token = verify_commit_certificate_token_v1(set, schedule, certificate)?;
        let expected = BftSigningJournalV1::for_schedule(
            token.next_height_schedule(),
            old.local_validator_id.clone(),
            old.local_signing_public_key.clone(),
        );
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
            let token = verify_commit_certificate_token_v1(set, schedule, certificate)?;
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
            BftSigningJournalEventV1::Proposal {
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
            BftSigningJournalEventV1::Vote {
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
            BftSigningJournalEventV1::Proposal { local: false, .. }
            | BftSigningJournalEventV1::Vote { local: false, .. }
            | BftSigningJournalEventV1::Timeout { .. } => {
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
            replay_bft_signing_journal_v1(next, set, schedule, timeouts)?;
            return Ok(schedule.clone());
        }
    }

    Err("BFT signing journal change is not one permitted durable transition".into())
}

pub fn complete_reserved_bft_proposal_v1(
    current: &BftSigningJournalV1,
    proposal: BftProposalV1,
    now_ms: i64,
    set: &BftValidatorSetV1,
    schedule: &BftHeightProposerScheduleV1,
    timeouts: BftTimeoutsV1,
) -> Result<BftSigningJournalV1, String> {
    replay_bft_signing_journal_v1(current, set, schedule, timeouts)?;
    let intent = current
        .pending_proposal_intent
        .as_ref()
        .ok_or_else(|| "no BFT proposal signing intent is pending".to_string())?;
    if !proposal_matches_intent(&proposal, intent) {
        return Err("signed BFT proposal does not match its pending intent".into());
    }
    let mut next = current.clone();
    next.events.push(BftSigningJournalEventV1::Proposal {
        proposal: Box::new(proposal),
        local: true,
        now_ms,
    });
    next.pending_proposal_intent = None;
    verify_bft_signing_journal_transition_v1(current, &next, set, schedule, timeouts)?;
    Ok(next)
}

pub fn complete_reserved_bft_vote_v1(
    current: &BftSigningJournalV1,
    vote: BftVoteV1,
    now_ms: i64,
    set: &BftValidatorSetV1,
    schedule: &BftHeightProposerScheduleV1,
    timeouts: BftTimeoutsV1,
) -> Result<BftSigningJournalV1, String> {
    replay_bft_signing_journal_v1(current, set, schedule, timeouts)?;
    let intent = current
        .pending_vote_intent
        .as_ref()
        .ok_or_else(|| "no BFT vote signing intent is pending".to_string())?;
    if vote_intent_from_vote(&vote) != *intent {
        return Err("signed BFT vote does not match its pending intent".into());
    }
    let mut next = current.clone();
    next.events.push(BftSigningJournalEventV1::Vote {
        vote: Box::new(vote),
        local: true,
        now_ms,
    });
    next.pending_vote_intent = None;
    verify_bft_signing_journal_transition_v1(current, &next, set, schedule, timeouts)?;
    Ok(next)
}

fn validate_static_binding(
    journal: &BftSigningJournalV1,
    set: &BftValidatorSetV1,
    schedule: &BftHeightProposerScheduleV1,
) -> Result<(), String> {
    let validator = set
        .validator(journal.local_validator_id.as_str())
        .ok_or_else(|| "BFT journal local signer is not in the validator set".to_string())?;
    if journal.schema_version != 4
        || journal.scope != *schedule.scope()
        || journal.scope != *set.scope()
        || journal.authority_epoch != schedule.authority_epoch()
        || journal.validator_set_hash != schedule.validator_set_hash()
        || journal.validator_set_hash != set.set_hash()
        || journal.height != schedule.height()
        || journal.parent_commit_ref != schedule.parent_commit_ref()
        || journal.starting_priority != *schedule.starting_priority().priorities()
        || journal.local_validator_id.trim().is_empty()
        || journal.local_signing_public_key.len() != 64
        || !journal
            .local_signing_public_key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || journal.local_signing_public_key != validator.public_key_hex
    {
        return Err("BFT signing journal differs from its authorized schedule or signer".into());
    }
    Ok(())
}

fn vote_intent_from_vote(vote: &BftVoteV1) -> BftVoteIntentV1 {
    BftVoteIntentV1 {
        height: vote.height,
        round: vote.round,
        phase: vote.phase,
        value: vote.value.clone(),
        validator_id: vote.validator_id.clone(),
    }
}

fn proposal_matches_intent(proposal: &BftProposalV1, intent: &BftProposalIntentV1) -> bool {
    let mut header = proposal.header.clone();
    header.signature_hex.clear();
    header == intent.header
        && proposal.block == intent.block
        && proposal.valid_round_evidence == intent.valid_round_evidence
}

fn hash_bounded<T: Serialize>(value: &T, domain: &[u8]) -> Result<(blake3::Hash, usize), String> {
    let mut writer = BoundedHasher::new(BFT_MAX_SIGNING_JOURNAL_BYTES_V1);
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

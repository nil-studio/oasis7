use oasis7_consensus::tendermint_v1::{
    BFT_MAX_SIGNING_JOURNAL_BYTES_V1, BftHeightProposerScheduleV1, BftSigningJournalV1,
    BftValidatorSetV1, replay_bft_signing_journal_v1, verify_bft_signing_journal_transition_v1,
    verify_commit_certificate_token_v1,
};
use oasis7_proto::distributed_bft::{
    BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1, BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1,
    BFT_MAX_SIGNED_MATERIAL_BYTES_V1,
};
use serde::{Deserialize, Serialize};

use super::binding::{BftSignerBindingV1, BftSignerError, invalid};
use super::wire::digest_hex;
use oasis7_proto::distributed_bft::BftCommitCertificateV1;

pub const BFT_SIGNER_MAX_HISTORY_BYTES_V1: usize = BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1;
pub const BFT_SIGNER_MAX_JOURNAL_BYTES_V1: usize = BFT_MAX_SIGNING_JOURNAL_BYTES_V1;
pub const BFT_SIGNER_MAX_FRAME_BYTES_V1: usize =
    2 * BFT_SIGNER_MAX_HISTORY_BYTES_V1 + 2 * BFT_MAX_SIGNED_MATERIAL_BYTES_V1 + 1024 * 1024;
const STATE_DOMAIN_V1: &str = "oasis7.bft.protected-signer.v1.state";
const CHECKPOINT_DOMAIN_V1: &str = "oasis7.bft.protected-signer.v1.checkpoint";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerCandidateV1 {
    pub history: Vec<BftCommitCertificateV1>,
    pub journal: BftSigningJournalV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerStateV1 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub revision: u64,
    pub history: Vec<BftCommitCertificateV1>,
    pub journal: BftSigningJournalV1,
    pub state_digest: String,
}

impl BftSignerStateV1 {
    pub fn genesis(binding: &BftSignerBindingV1) -> Result<Self, BftSignerError> {
        let set = binding.validate()?;
        let schedule = BftHeightProposerScheduleV1::genesis(&set, binding.authority_epoch)
            .map_err(|error| invalid(format!("BFT signer genesis schedule: {error}")))?;
        let journal = BftSigningJournalV1::for_schedule(
            &schedule,
            binding.local_validator_id.clone(),
            binding.local_signing_public_key.clone(),
        );
        Self::from_candidate(binding, 0, Vec::new(), journal)
    }

    pub(crate) fn from_candidate(
        binding: &BftSignerBindingV1,
        revision: u64,
        history: Vec<BftCommitCertificateV1>,
        journal: BftSigningJournalV1,
    ) -> Result<Self, BftSignerError> {
        let mut state = Self {
            schema_version: 1,
            binding_digest: binding.digest()?,
            revision,
            history,
            journal,
            state_digest: String::new(),
        };
        state.state_digest = digest_hex(STATE_DOMAIN_V1, &StateDigestInput::from(&state))?;
        validate_state(binding, &state)?;
        Ok(state)
    }

    pub fn candidate(&self) -> BftSignerCandidateV1 {
        BftSignerCandidateV1 {
            history: self.history.clone(),
            journal: self.journal.clone(),
        }
    }

    pub fn validate(&self, binding: &BftSignerBindingV1) -> Result<(), BftSignerError> {
        validate_state(binding, self).map(|_| ())
    }
}

#[derive(Serialize)]
struct StateDigestInput<'a> {
    schema_version: u32,
    binding_digest: &'a str,
    revision: u64,
    history: &'a [BftCommitCertificateV1],
    journal: &'a BftSigningJournalV1,
}

impl<'a> From<&'a BftSignerStateV1> for StateDigestInput<'a> {
    fn from(state: &'a BftSignerStateV1) -> Self {
        Self {
            schema_version: state.schema_version,
            binding_digest: &state.binding_digest,
            revision: state.revision,
            history: &state.history,
            journal: &state.journal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftProtectedCheckpointV1 {
    pub state: BftSignerStateV1,
    pub lease_generation: u64,
    pub challenge: String,
    pub request_digest: String,
    pub provider_public_key: String,
    pub provider_signature_hex: String,
}

impl BftProtectedCheckpointV1 {
    pub(crate) fn signable_body(&self) -> CheckpointProofInput<'_> {
        CheckpointProofInput {
            state: &self.state,
            lease_generation: self.lease_generation,
            challenge: &self.challenge,
            request_digest: &self.request_digest,
        }
    }

    pub(crate) fn validate(
        &self,
        binding: &BftSignerBindingV1,
        expected_challenge: &str,
        expected_request_digest: &str,
        expected_generation: u64,
        provider_public_key: &str,
    ) -> Result<(), BftSignerError> {
        if self.challenge != expected_challenge
            || self.request_digest != expected_request_digest
            || self.lease_generation != expected_generation
            || self.provider_public_key != provider_public_key
        {
            return Err(invalid(
                "BFT protected checkpoint freshness or lease binding",
            ));
        }
        self.state.validate(binding)?;
        super::binding::verify_signature(
            CHECKPOINT_DOMAIN_V1,
            &self.signable_body(),
            &self.provider_signature_hex,
            provider_public_key,
        )?;
        Ok(())
    }
}

#[derive(Serialize)]
pub(crate) struct CheckpointProofInput<'a> {
    pub state: &'a BftSignerStateV1,
    pub lease_generation: u64,
    pub challenge: &'a str,
    pub request_digest: &'a str,
}

pub(crate) fn validate_state(
    binding: &BftSignerBindingV1,
    state: &BftSignerStateV1,
) -> Result<BftHeightProposerScheduleV1, BftSignerError> {
    let set = binding.validate()?;
    if state.schema_version != 1 || state.binding_digest != binding.digest()? {
        return Err(invalid("BFT signer state schema or binding mismatch"));
    }
    validate_history(&state.history)?;
    let journal_bytes = serde_cbor::to_vec(&state.journal)
        .map_err(|error| invalid(format!("encode BFT signer journal: {error}")))?;
    if journal_bytes.len() > BFT_SIGNER_MAX_JOURNAL_BYTES_V1 {
        return Err(invalid("BFT signer journal byte bound"));
    }
    let digest = digest_hex(STATE_DOMAIN_V1, &StateDigestInput::from(state))?;
    if digest != state.state_digest {
        return Err(invalid("BFT signer state digest mismatch"));
    }

    let mut schedule = BftHeightProposerScheduleV1::genesis(&set, binding.authority_epoch)
        .map_err(|error| invalid(format!("BFT signer genesis schedule: {error}")))?;
    for certificate in &state.history {
        let verified = verify_commit_certificate_token_v1(&set, &schedule, certificate)
            .map_err(|error| invalid(format!("BFT signer commit history: {error}")))?;
        schedule = verified.next_height_schedule().clone();
    }
    validate_journal_schedule(binding, &state.journal, &set, &schedule)?;
    replay_bft_signing_journal_v1(&state.journal, &set, &schedule, binding.timeouts)
        .map_err(|error| invalid(format!("BFT signer journal replay: {error}")))?;
    Ok(schedule)
}

pub(crate) fn validate_transition(
    binding: &BftSignerBindingV1,
    previous: &BftSignerStateV1,
    candidate: &BftSignerCandidateV1,
    protected_now_ms: i64,
) -> Result<BftSignerStateV1, BftSignerError> {
    validate_transition_inner(binding, previous, candidate, protected_now_ms, false)
}

pub(crate) fn validate_height_timer_start(
    binding: &BftSignerBindingV1,
    previous: &BftSignerStateV1,
    protected_now_ms: i64,
) -> Result<BftSignerStateV1, BftSignerError> {
    if protected_now_ms < 0
        || previous.journal.height_started_at_ms.is_some()
        || !previous.journal.events().is_empty()
        || previous.journal.pending_proposal_intent().is_some()
        || previous.journal.pending_vote_intent().is_some()
        || previous.journal.pending_commit().is_some()
    {
        return Err(invalid(
            "BFT height timer can only start once on an empty height",
        ));
    }
    let mut candidate = previous.candidate();
    candidate.journal.height_started_at_ms = Some(protected_now_ms);
    validate_transition_inner(binding, previous, &candidate, protected_now_ms, true)
}

fn validate_transition_inner(
    binding: &BftSignerBindingV1,
    previous: &BftSignerStateV1,
    candidate: &BftSignerCandidateV1,
    protected_now_ms: i64,
    allow_timer_start: bool,
) -> Result<BftSignerStateV1, BftSignerError> {
    let old_schedule = validate_state(binding, previous)?;
    let set = binding.validate()?;
    let revision = previous
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("BFT signer revision overflow"))?;
    let next_schedule = verify_bft_signing_journal_transition_v1(
        &previous.journal,
        &candidate.journal,
        &set,
        &old_schedule,
        binding.timeouts,
    )
    .map_err(|error| invalid(format!("BFT signer journal transition: {error}")))?;
    let old_height = previous.journal.height;
    let next_height = candidate.journal.height;
    if old_height == next_height {
        let old_started_at = previous.journal.height_started_at_ms;
        let next_started_at = candidate.journal.height_started_at_ms;
        let valid_timer_start = allow_timer_start
            && old_started_at.is_none()
            && next_started_at == Some(protected_now_ms);
        if old_started_at != next_started_at && !valid_timer_start {
            return Err(invalid(
                "BFT height timer must be started by the protected signer",
            ));
        }
    }
    let mut history = previous.history.clone();
    match next_height.cmp(&old_height) {
        std::cmp::Ordering::Equal => {
            if candidate.history != previous.history {
                return Err(invalid(
                    "BFT signer history changed without a committed height",
                ));
            }
        }
        std::cmp::Ordering::Greater => {
            let expected_next_height = old_height
                .checked_add(1)
                .ok_or_else(|| invalid("BFT signer height overflow"))?;
            if next_height != expected_next_height
                || candidate.journal
                    != BftSigningJournalV1::for_schedule(
                        &next_schedule,
                        binding.local_validator_id.clone(),
                        binding.local_signing_public_key.clone(),
                    )
            {
                return Err(invalid(
                    "BFT signer next-height journal is not empty successor",
                ));
            }
            let certificate = previous
                .journal
                .pending_commit()
                .ok_or_else(|| invalid("BFT signer next height has no pending commit"))?;
            history.push(certificate.clone());
            if candidate.history != history {
                return Err(invalid(
                    "BFT signer committed history does not append exact QC",
                ));
            }
        }
        std::cmp::Ordering::Less => {
            return Err(invalid("BFT signer journal height regressed"));
        }
    }
    let next =
        BftSignerStateV1::from_candidate(binding, revision, history, candidate.journal.clone())?;
    validate_event_time_transition(previous, &next, protected_now_ms)?;
    Ok(next)
}

fn validate_journal_schedule(
    binding: &BftSignerBindingV1,
    journal: &BftSigningJournalV1,
    set: &BftValidatorSetV1,
    schedule: &BftHeightProposerScheduleV1,
) -> Result<(), BftSignerError> {
    if journal.scope != *set.scope()
        || journal.authority_epoch != binding.authority_epoch
        || journal.validator_set_hash != set.set_hash()
        || journal.height != schedule.height()
        || journal.parent_commit_ref != schedule.parent_commit_ref()
        || journal.starting_priority != *schedule.starting_priority().priorities()
        || journal.local_validator_id != binding.local_validator_id
        || journal.local_signing_public_key != binding.local_signing_public_key
    {
        return Err(invalid(
            "BFT signer journal does not match reconstructed schedule",
        ));
    }
    Ok(())
}

fn validate_event_time_transition(
    previous: &BftSignerStateV1,
    next: &BftSignerStateV1,
    protected_now_ms: i64,
) -> Result<(), BftSignerError> {
    let old_time = previous
        .journal
        .events()
        .last()
        .map(|event| event.event_time_ms())
        .unwrap_or(previous.journal.height_started_at_ms.unwrap_or(i64::MIN));
    let mut prior_event_time = old_time;
    for event in next
        .journal
        .events()
        .iter()
        .skip(previous.journal.events().len())
    {
        let time = event.event_time_ms();
        if time < prior_event_time || time > protected_now_ms {
            return Err(invalid(
                "BFT signer journal event time is outside protected interval",
            ));
        }
        prior_event_time = time;
    }
    Ok(())
}

pub(crate) fn candidate_event_time(
    previous: &BftSignerStateV1,
    candidate: &BftSignerCandidateV1,
) -> Option<i64> {
    candidate
        .journal
        .events()
        .get(previous.journal.events().len())
        .map(|event| event.event_time_ms())
}

pub(crate) fn validate_candidate_bounds(
    candidate: &BftSignerCandidateV1,
) -> Result<(), BftSignerError> {
    if candidate.history.len() > BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 {
        return Err(invalid("BFT signer commit history height bound"));
    }
    let history = serde_cbor::to_vec(&candidate.history)
        .map_err(|error| invalid(format!("encode BFT signer history: {error}")))?;
    if history.len() > BFT_SIGNER_MAX_HISTORY_BYTES_V1 {
        return Err(invalid("BFT signer commit history byte bound"));
    }
    let journal = serde_cbor::to_vec(&candidate.journal)
        .map_err(|error| invalid(format!("encode BFT signer journal: {error}")))?;
    if journal.len() > BFT_SIGNER_MAX_JOURNAL_BYTES_V1 {
        return Err(invalid("BFT signer journal byte bound"));
    }
    Ok(())
}

pub(crate) fn validate_history(history: &[BftCommitCertificateV1]) -> Result<(), BftSignerError> {
    if history.len() > BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 {
        return Err(invalid("BFT signer commit history height bound"));
    }
    let bytes = serde_cbor::to_vec(&history)
        .map_err(|error| invalid(format!("encode BFT signer history: {error}")))?;
    if bytes.len() > BFT_SIGNER_MAX_HISTORY_BYTES_V1 {
        return Err(invalid("BFT signer commit history byte bound"));
    }
    Ok(())
}

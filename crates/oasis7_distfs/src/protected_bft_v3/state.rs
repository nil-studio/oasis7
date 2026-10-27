//! Public-only protected state rooted in an independently admitted H schedule.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::crypto::{
    BFT_SIGNER_MAX_FRAME_BYTES_V3, BFT_SIGNER_MAX_HISTORY_BYTES_V3,
    BFT_SIGNER_MAX_JOURNAL_BYTES_V3, digest_hex, encode_bounded, verify_signature,
};
use oasis7_consensus::tendermint_v3::{
    BFT_MAX_SIGNING_JOURNAL_BYTES_V3, BftSigningJournalV3, replay_bft_signing_journal_v3,
    verify_bft_signing_journal_transition_v3, verify_reserved_bft_commit_transition_v3,
};
use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3, FormalBftCertifiedHeaderV3, verify_formal_bft_header_certificate_v3,
};
use serde::{Deserialize, Serialize};
const STATE_DOMAIN_V3: &str = "oasis7.bft.protected-signer.v3.state";
const CHECKPOINT_DOMAIN_V3: &str = "oasis7.bft.protected-signer.v3.checkpoint";
pub const BFT_SIGNER_MAX_HISTORY_HEIGHTS_V3: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftDriverApplyIntentV3 {
    pub height: u64,
    pub header_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftDriverAppliedReceiptV3 {
    pub height: u64,
    pub header_id: String,
    pub driver_install_digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerStateV3 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub revision: u64,
    #[serde(deserialize_with = "bounded_history")]
    pub history: Vec<FormalBftCertifiedHeaderV3>,
    pub journal: Option<BftSigningJournalV3>,
    pub pending_driver_apply: Option<BftDriverApplyIntentV3>,
    #[serde(deserialize_with = "bounded_receipts")]
    pub applied_installs: Vec<BftDriverAppliedReceiptV3>,
    pub state_digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerCandidateV3 {
    #[serde(deserialize_with = "bounded_history")]
    pub history: Vec<FormalBftCertifiedHeaderV3>,
    pub journal: Option<BftSigningJournalV3>,
    pub pending_driver_apply: Option<BftDriverApplyIntentV3>,
    #[serde(deserialize_with = "bounded_receipts")]
    pub applied_installs: Vec<BftDriverAppliedReceiptV3>,
}
impl BftSignerStateV3 {
    pub fn from_handoff(binding: &BftSignerBindingV3) -> Result<Self, BftSignerErrorV3> {
        binding.validate()?;
        let journal = journal_for_schedule(binding, binding.initial_schedule())?;
        Self::from_candidate(
            binding,
            0,
            BftSignerCandidateV3 {
                history: Vec::new(),
                journal,
                pending_driver_apply: None,
                applied_installs: Vec::new(),
            },
        )
    }
    pub(crate) fn from_candidate(
        binding: &BftSignerBindingV3,
        revision: u64,
        candidate: BftSignerCandidateV3,
    ) -> Result<Self, BftSignerErrorV3> {
        let mut state = Self {
            schema_version: 3,
            binding_digest: binding.digest()?,
            revision,
            history: candidate.history,
            journal: candidate.journal,
            pending_driver_apply: candidate.pending_driver_apply,
            applied_installs: candidate.applied_installs,
            state_digest: String::new(),
        };
        state.state_digest = digest_hex(STATE_DOMAIN_V3, &StateDigestInput::from(&state))?;
        validate_state(binding, None, &state)?;
        Ok(state)
    }
    pub fn candidate(&self) -> BftSignerCandidateV3 {
        BftSignerCandidateV3 {
            history: self.history.clone(),
            journal: self.journal.clone(),
            pending_driver_apply: self.pending_driver_apply.clone(),
            applied_installs: self.applied_installs.clone(),
        }
    }
    pub fn validate(
        &self,
        binding: &BftSignerBindingV3,
        provider_public_key: &str,
    ) -> Result<(), BftSignerErrorV3> {
        validate_state(binding, Some(provider_public_key), self).map(|_| ())
    }
    pub fn encoded_len(&self) -> Result<usize, BftSignerErrorV3> {
        encode_bounded(self, BFT_SIGNER_MAX_FRAME_BYTES_V3).map(|b| b.len())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VerifiedBftSignerStateV3 {
    pub(crate) state: BftSignerStateV3,
    pub(crate) schedule: BftHeaderHeightScheduleV3,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerCheckpointV3 {
    pub state: BftSignerStateV3,
    pub lease_generation: u64,
    pub challenge: String,
    pub request_digest: String,
    pub provider_public_key: String,
    pub provider_signature_hex: String,
}
impl BftSignerCheckpointV3 {
    pub(crate) fn signable_body(&self) -> CheckpointProofInputV3<'_> {
        CheckpointProofInputV3 {
            state: &self.state,
            lease_generation: self.lease_generation,
            challenge: &self.challenge,
            request_digest: &self.request_digest,
        }
    }
    pub(crate) fn validate(
        &self,
        binding: &BftSignerBindingV3,
        expected_challenge: &str,
        expected_request_digest: &str,
        expected_generation: u64,
        provider_public_key: &str,
    ) -> Result<VerifiedBftSignerStateV3, BftSignerErrorV3> {
        if self.challenge != expected_challenge
            || self.request_digest != expected_request_digest
            || self.lease_generation != expected_generation
            || self.provider_public_key != provider_public_key
        {
            return Err(invalid("BFT V3 checkpoint freshness or lease binding"));
        }
        let verified = validate_state(binding, Some(provider_public_key), &self.state)?;
        verify_signature(
            CHECKPOINT_DOMAIN_V3,
            &self.signable_body(),
            &self.provider_signature_hex,
            provider_public_key,
        )?;
        Ok(verified)
    }
}
#[derive(Serialize)]
pub(crate) struct CheckpointProofInputV3<'a> {
    state: &'a BftSignerStateV3,
    lease_generation: u64,
    challenge: &'a str,
    request_digest: &'a str,
}
#[derive(Serialize)]
struct StateDigestInput<'a> {
    schema_version: u32,
    binding_digest: &'a str,
    revision: u64,
    history: &'a [FormalBftCertifiedHeaderV3],
    journal: &'a Option<BftSigningJournalV3>,
    pending_driver_apply: &'a Option<BftDriverApplyIntentV3>,
    applied_installs: &'a [BftDriverAppliedReceiptV3],
}
impl<'a> From<&'a BftSignerStateV3> for StateDigestInput<'a> {
    fn from(s: &'a BftSignerStateV3) -> Self {
        Self {
            schema_version: s.schema_version,
            binding_digest: &s.binding_digest,
            revision: s.revision,
            history: &s.history,
            journal: &s.journal,
            pending_driver_apply: &s.pending_driver_apply,
            applied_installs: &s.applied_installs,
        }
    }
}
pub(crate) fn validate_state(
    binding: &BftSignerBindingV3,
    _provider_public_key: Option<&str>,
    state: &BftSignerStateV3,
) -> Result<VerifiedBftSignerStateV3, BftSignerErrorV3> {
    binding.validate()?;
    if state.schema_version != 3 || state.binding_digest != binding.digest()? {
        return Err(invalid("BFT V3 state schema or H binding mismatch"));
    }
    if state.history.len() > BFT_SIGNER_MAX_HISTORY_HEIGHTS_V3 {
        return Err(invalid("BFT V3 history height capacity"));
    }
    encode_bounded(&state.history, BFT_SIGNER_MAX_HISTORY_BYTES_V3)?;
    if digest_hex(STATE_DOMAIN_V3, &StateDigestInput::from(state))? != state.state_digest {
        return Err(invalid("BFT V3 state digest mismatch"));
    }
    let mut schedule = binding.initial_schedule().clone();
    for certificate in &state.history {
        if certificate.execution_commitment.transition.is_some()
            || certificate.execution_commitment.transition_ref.is_some()
        {
            return Err(invalid(
                "BFT V3 provider membership transitions are not installed",
            ));
        }
        schedule = verify_formal_bft_header_certificate_v3(&schedule, certificate)
            .map_err(invalid)?
            .next_height_schedule()
            .clone();
    }
    if state.applied_installs.len() > state.history.len() {
        return Err(invalid(
            "BFT V3 applied receipt history exceeds certificates",
        ));
    }
    for (receipt, certificate) in state.applied_installs.iter().zip(&state.history) {
        validate_digest(&receipt.driver_install_digest)?;
        if receipt.height != certificate.execution_commitment.height
            || receipt.header_id != certificate.header.block_id().map_err(invalid)?
        {
            return Err(invalid(
                "BFT V3 applied receipt differs from original header",
            ));
        }
    }
    match (
        &state.pending_driver_apply,
        state.history.len() - state.applied_installs.len(),
    ) {
        (None, 0) => {}
        (Some(pending), 1) => {
            let tip = state
                .history
                .last()
                .ok_or_else(|| invalid("BFT V3 pending apply without QC"))?;
            if pending.height != tip.execution_commitment.height
                || pending.header_id != tip.header.block_id().map_err(invalid)?
            {
                return Err(invalid("BFT V3 pending apply differs from QC tip"));
            }
        }
        _ => {
            return Err(invalid(
                "BFT V3 driver apply must confirm each original QC before the next",
            ));
        }
    }
    let validator = schedule
        .active_set()
        .validator(&binding.local_validator_id)
        .filter(|v| {
            binding
                .provisioned_signing_keys
                .iter()
                .any(|k| k.validator_id == v.validator_id && k.public_key_hex == v.public_key_hex)
        });
    match (&state.journal, validator) {
        (Some(journal), Some(v)) => {
            if journal.local_validator_id() != binding.local_validator_id
                || journal.local_signing_public_key() != v.public_key_hex
            {
                return Err(invalid(
                    "BFT V3 journal identity differs from active provisioned key",
                ));
            }
            if journal.encoded_len().map_err(invalid)?
                > BFT_SIGNER_MAX_JOURNAL_BYTES_V3.min(BFT_MAX_SIGNING_JOURNAL_BYTES_V3)
            {
                return Err(invalid("BFT V3 journal capacity"));
            }
            replay_bft_signing_journal_v3(
                journal,
                schedule.active_set(),
                &schedule,
                binding.timeouts,
            )
            .map_err(invalid)?;
        }
        (None, None) => {}
        _ => return Err(invalid("BFT V3 active signer journal missing or foreign")),
    }
    Ok(VerifiedBftSignerStateV3 {
        state: state.clone(),
        schedule,
    })
}
pub(super) fn journal_for_schedule(
    binding: &BftSignerBindingV3,
    schedule: &BftHeaderHeightScheduleV3,
) -> Result<Option<BftSigningJournalV3>, BftSignerErrorV3> {
    let Some(v) = schedule.active_set().validator(&binding.local_validator_id) else {
        return Ok(None);
    };
    if !binding
        .provisioned_signing_keys
        .iter()
        .any(|k| k.validator_id == v.validator_id && k.public_key_hex == v.public_key_hex)
    {
        return Ok(None);
    }
    BftSigningJournalV3::for_schedule(
        schedule,
        binding.local_validator_id.clone(),
        v.public_key_hex.clone(),
    )
    .map(Some)
    .map_err(invalid)
}
pub(super) fn validate_signing_install_gate(
    binding: &BftSignerBindingV3,
    state: &BftSignerStateV3,
    schedule: &BftHeaderHeightScheduleV3,
) -> Result<(), BftSignerErrorV3> {
    if state.pending_driver_apply.is_some() {
        return Err(invalid(
            "BFT V3 original driver installation is not confirmed",
        ));
    }
    if schedule.pending_transition().is_some() || schedule.last_activation_height() > 0 {
        return Err(invalid("BFT V3 membership installation unsupported"));
    }
    if !schedule
        .active_set()
        .validator(&binding.local_validator_id)
        .is_some_and(|v| {
            binding
                .provisioned_signing_keys
                .iter()
                .any(|k| k.validator_id == v.validator_id && k.public_key_hex == v.public_key_hex)
        })
    {
        return Err(invalid("BFT V3 local signing key is not active"));
    }
    Ok(())
}
pub(crate) fn validate_candidate_transition(
    binding: &BftSignerBindingV3,
    previous: &VerifiedBftSignerStateV3,
    candidate: &BftSignerCandidateV3,
    _provider: &str,
) -> Result<VerifiedBftSignerStateV3, BftSignerErrorV3> {
    let old = &previous.state;
    if candidate.history.len() < old.history.len()
        || candidate.history.get(..old.history.len()) != Some(old.history.as_slice())
    {
        return Err(invalid("BFT V3 history is not an exact prefix extension"));
    }
    let added = candidate.history.len() - old.history.len();
    if added > 1 {
        return Err(invalid("BFT V3 candidate appends more than one QC"));
    }
    if added == 1 {
        if old.pending_driver_apply.is_some() || candidate.applied_installs != old.applied_installs
        {
            return Err(invalid("BFT V3 unconfirmed driver install blocks new QC"));
        }
        let certificate = candidate
            .history
            .last()
            .ok_or_else(|| invalid("BFT V3 missing append QC"))?;
        let verified = verify_formal_bft_header_certificate_v3(&previous.schedule, certificate)
            .map_err(invalid)?;
        let expected = journal_for_schedule(binding, verified.next_height_schedule())?;
        if candidate.journal != expected
            || candidate.pending_driver_apply
                != Some(BftDriverApplyIntentV3 {
                    height: certificate.execution_commitment.height,
                    header_id: verified.block_id().into(),
                })
        {
            return Err(invalid(
                "BFT V3 QC append did not retain exact pending driver install",
            ));
        }
        if let Some(journal) = &old.journal {
            if journal.pending_commit() != Some(certificate) {
                return Err(invalid("BFT V3 QC differs from exact reserved commit"));
            }
            let next = verify_reserved_bft_commit_transition_v3(
                journal,
                certificate,
                previous.schedule.active_set(),
                &previous.schedule,
                binding.timeouts,
            )
            .map_err(invalid)?;
            if &next != verified.next_height_schedule() {
                return Err(invalid("BFT V3 reserved commit schedule differs"));
            }
        }
    } else if let Some(pending) = &old.pending_driver_apply {
        if candidate.history != old.history
            || candidate.journal != old.journal
            || candidate.pending_driver_apply.is_some()
            || candidate.applied_installs.len() != old.applied_installs.len() + 1
            || !candidate
                .applied_installs
                .starts_with(&old.applied_installs)
            || !candidate
                .applied_installs
                .last()
                .is_some_and(|r| r.height == pending.height && r.header_id == pending.header_id)
        {
            return Err(invalid(
                "BFT V3 pending apply can only finish its exact original receipt",
            ));
        }
    } else {
        if candidate.pending_driver_apply != old.pending_driver_apply
            || candidate.applied_installs != old.applied_installs
            || candidate.history != old.history
        {
            return Err(invalid("BFT V3 protocol advance changed install state"));
        }
        match (&old.journal, &candidate.journal) {
            (Some(a), Some(b)) if a != b => verify_bft_signing_journal_transition_v3(
                a,
                b,
                previous.schedule.active_set(),
                &previous.schedule,
                binding.timeouts,
            )
            .map_err(invalid)?,
            _ => {
                return Err(invalid(
                    "BFT V3 candidate is not one legal journal transition",
                ));
            }
        }
    }
    let revision = old
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("BFT V3 revision overflow"))?;
    let next = BftSignerStateV3::from_candidate(binding, revision, candidate.clone())?;
    validate_state(binding, None, &next)
}
pub(crate) fn validate_digest(value: &str) -> Result<(), BftSignerErrorV3> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "BFT V3 driver install digest must be canonical BLAKE3",
        ));
    }
    Ok(())
}

fn bounded_history<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<FormalBftCertifiedHeaderV3>, D::Error> {
    bounded_sequence(d)
}
fn bounded_receipts<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<BftDriverAppliedReceiptV3>, D::Error> {
    bounded_sequence(d)
}
fn bounded_sequence<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    struct Bound<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Bound<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 64 protected header records")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq
                .size_hint()
                .is_some_and(|n| n > BFT_SIGNER_MAX_HISTORY_HEIGHTS_V3)
            {
                return Err(serde::de::Error::custom(
                    "BFT V3 retained record count exceeds bound",
                ));
            }
            let mut values = Vec::new();
            while values.len() < BFT_SIGNER_MAX_HISTORY_HEIGHTS_V3 {
                let Some(value) = seq.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom(
                    "BFT V3 retained record count exceeds bound",
                ));
            }
            Ok(values)
        }
    }
    d.deserialize_seq(Bound(std::marker::PhantomData))
}

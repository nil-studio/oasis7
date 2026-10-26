use oasis7_consensus::tendermint_v2::{
    BFT_MAX_SIGNING_JOURNAL_BYTES_V2, BftHeightProposerScheduleV2, BftSigningJournalV2,
    BftValidatorSetV2, replay_bft_signing_journal_v2, verify_bft_signing_journal_transition_v2,
    verify_commit_certificate_token_v2, verify_reserved_bft_commit_transition_v2,
};
use oasis7_proto::distributed_bft::BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1;
use oasis7_proto::distributed_bft::v2::{BftCommitCertificateV2, BftValidatorTransitionV2};
use serde::{Deserialize, Serialize};

use super::binding::{BftSignerBindingV2, BftSignerErrorV2, invalid};
use super::crypto::{
    BFT_SIGNER_MAX_FRAME_BYTES_V2, BFT_SIGNER_MAX_HISTORY_BYTES_V2,
    BFT_SIGNER_MAX_INSTALL_HISTORY_BYTES_V2, BFT_SIGNER_MAX_INSTALL_RECORD_BYTES_V2,
    BFT_SIGNER_MAX_JOURNAL_BYTES_V2, digest_hex, encode_bounded, verify_signature,
};
use super::install::{BftTransitionInstallIntentV2, BftTransitionInstallReceiptV2};

const STATE_DOMAIN_V2: &str = "oasis7.bft.protected-signer.v2.state";
const CHECKPOINT_DOMAIN_V2: &str = "oasis7.bft.protected-signer.v2.checkpoint";
pub const BFT_SIGNER_MAX_HISTORY_HEIGHTS_V2: usize = BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerStateV2 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub revision: u64,
    pub history: Vec<BftCommitCertificateV2>,
    pub journal: Option<BftSigningJournalV2>,
    pub pending_install: Option<BftTransitionInstallIntentV2>,
    pub install_receipts: Vec<BftTransitionInstallReceiptV2>,
    pub state_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerCandidateV2 {
    pub history: Vec<BftCommitCertificateV2>,
    pub journal: Option<BftSigningJournalV2>,
    pub pending_install: Option<BftTransitionInstallIntentV2>,
    pub install_receipts: Vec<BftTransitionInstallReceiptV2>,
}

impl BftSignerStateV2 {
    pub fn genesis(binding: &BftSignerBindingV2) -> Result<Self, BftSignerErrorV2> {
        let set = binding.validate()?;
        let schedule = BftHeightProposerScheduleV2::genesis(
            set,
            binding.schedule_policy.clone(),
            binding.genesis_authority_epoch,
            binding.genesis_state_root.clone(),
            binding.genesis_manifest_hash.clone(),
        )
        .map_err(|error| invalid(format!("BFT V2 signer genesis schedule: {error}")))?;
        let journal = journal_for_schedule(binding, &schedule)?;
        Self::from_candidate(binding, 0, Vec::new(), journal, None, Vec::new(), None)
    }

    pub(crate) fn from_candidate(
        binding: &BftSignerBindingV2,
        revision: u64,
        history: Vec<BftCommitCertificateV2>,
        journal: Option<BftSigningJournalV2>,
        pending_install: Option<BftTransitionInstallIntentV2>,
        install_receipts: Vec<BftTransitionInstallReceiptV2>,
        provider_public_key: Option<&str>,
    ) -> Result<Self, BftSignerErrorV2> {
        let mut state = Self {
            schema_version: 2,
            binding_digest: binding.digest()?,
            revision,
            history,
            journal,
            pending_install,
            install_receipts,
            state_digest: String::new(),
        };
        state.state_digest = digest_hex(STATE_DOMAIN_V2, &StateDigestInput::from(&state))?;
        validate_state(binding, provider_public_key, &state)?;
        Ok(state)
    }

    pub fn candidate(&self) -> BftSignerCandidateV2 {
        BftSignerCandidateV2 {
            history: self.history.clone(),
            journal: self.journal.clone(),
            pending_install: self.pending_install.clone(),
            install_receipts: self.install_receipts.clone(),
        }
    }

    pub fn validate(
        &self,
        binding: &BftSignerBindingV2,
        provider_public_key: &str,
    ) -> Result<(), BftSignerErrorV2> {
        validate_state(binding, Some(provider_public_key), self).map(|_| ())
    }

    pub fn encoded_len(&self) -> Result<usize, BftSignerErrorV2> {
        encode_bounded(self, BFT_SIGNER_MAX_FRAME_BYTES_V2).map(|bytes| bytes.len())
    }

    pub fn install_receipt_for_request(
        &self,
        request_id: &str,
    ) -> Option<&BftTransitionInstallReceiptV2> {
        self.install_receipts
            .iter()
            .find(|receipt| receipt.body.request_id == request_id)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VerifiedBftSignerStateV2 {
    pub(crate) state: BftSignerStateV2,
    pub(crate) schedule: BftHeightProposerScheduleV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerCheckpointV2 {
    pub state: BftSignerStateV2,
    pub lease_generation: u64,
    pub challenge: String,
    pub request_digest: String,
    pub provider_public_key: String,
    pub provider_signature_hex: String,
}

impl BftSignerCheckpointV2 {
    pub(crate) fn signable_body(&self) -> CheckpointProofInputV2<'_> {
        CheckpointProofInputV2 {
            state: &self.state,
            lease_generation: self.lease_generation,
            challenge: &self.challenge,
            request_digest: &self.request_digest,
        }
    }

    pub(crate) fn validate(
        &self,
        binding: &BftSignerBindingV2,
        expected_challenge: &str,
        expected_request_digest: &str,
        expected_generation: u64,
        provider_public_key: &str,
    ) -> Result<VerifiedBftSignerStateV2, BftSignerErrorV2> {
        if self.challenge != expected_challenge
            || self.request_digest != expected_request_digest
            || self.lease_generation != expected_generation
            || self.provider_public_key != provider_public_key
        {
            return Err(invalid("BFT V2 checkpoint freshness or lease binding"));
        }
        let verified = validate_state(binding, Some(provider_public_key), &self.state)?;
        verify_signature(
            CHECKPOINT_DOMAIN_V2,
            &self.signable_body(),
            &self.provider_signature_hex,
            provider_public_key,
        )?;
        Ok(verified)
    }
}

#[derive(Serialize)]
pub(crate) struct CheckpointProofInputV2<'a> {
    state: &'a BftSignerStateV2,
    lease_generation: u64,
    challenge: &'a str,
    request_digest: &'a str,
}

#[derive(Serialize)]
struct StateDigestInput<'a> {
    schema_version: u32,
    binding_digest: &'a str,
    revision: u64,
    history: &'a [BftCommitCertificateV2],
    journal: &'a Option<BftSigningJournalV2>,
    pending_install: &'a Option<BftTransitionInstallIntentV2>,
    install_receipts: &'a [BftTransitionInstallReceiptV2],
}

impl<'a> From<&'a BftSignerStateV2> for StateDigestInput<'a> {
    fn from(state: &'a BftSignerStateV2) -> Self {
        Self {
            schema_version: state.schema_version,
            binding_digest: &state.binding_digest,
            revision: state.revision,
            history: &state.history,
            journal: &state.journal,
            pending_install: &state.pending_install,
            install_receipts: &state.install_receipts,
        }
    }
}

pub(crate) fn validate_state(
    binding: &BftSignerBindingV2,
    provider_public_key: Option<&str>,
    state: &BftSignerStateV2,
) -> Result<VerifiedBftSignerStateV2, BftSignerErrorV2> {
    if state.schema_version != 2 || state.binding_digest != binding.digest()? {
        return Err(invalid("BFT V2 signer state schema or binding mismatch"));
    }
    if state.history.len() > BFT_SIGNER_MAX_HISTORY_HEIGHTS_V2 {
        return Err(invalid("BFT V2 signer commit history height bound"));
    }
    encode_bounded(&state.history, BFT_SIGNER_MAX_HISTORY_BYTES_V2)?;
    if let Some(journal) = &state.journal {
        let encoded_len = journal
            .encoded_len()
            .map_err(|error| invalid(format!("BFT V2 journal encoding: {error}")))?;
        if encoded_len > BFT_SIGNER_MAX_JOURNAL_BYTES_V2
            || encoded_len > BFT_MAX_SIGNING_JOURNAL_BYTES_V2
        {
            return Err(invalid("BFT V2 signer journal byte bound"));
        }
    }
    let digest = digest_hex(STATE_DOMAIN_V2, &StateDigestInput::from(state))?;
    if digest != state.state_digest {
        return Err(invalid("BFT V2 signer state digest mismatch"));
    }

    let (schedule, transition_contexts) = reconstruct_install_contexts(binding, &state.history)?;
    validate_install_records(binding, provider_public_key, state)?;
    validate_installed_transitions(binding, &transition_contexts, &schedule, state)?;
    let active_local = schedule
        .active_set()
        .validator(&binding.local_validator_id)
        .filter(|validator| {
            binding.provisioned_signing_keys.iter().any(|key| {
                key.validator_id == validator.validator_id
                    && key.public_key_hex == validator.public_key_hex
            })
        });
    match (&state.journal, active_local) {
        (Some(journal), Some(validator)) => {
            if journal.local_validator_id() != binding.local_validator_id
                || journal.local_signing_public_key() != validator.public_key_hex
            {
                return Err(invalid(
                    "BFT V2 signer journal identity is not the active pinned key",
                ));
            }
            replay_bft_signing_journal_v2(
                journal,
                schedule.active_set(),
                &schedule,
                binding.timeouts,
            )
            .map_err(|error| invalid(format!("BFT V2 signer journal replay: {error}")))?;
        }
        (None, None) => {}
        (Some(_), None) => {
            return Err(invalid(
                "BFT V2 signer journal exists for an inactive local key",
            ));
        }
        (None, Some(_)) => {
            return Err(invalid("BFT V2 active local signer is missing its journal"));
        }
    }
    Ok(VerifiedBftSignerStateV2 {
        state: state.clone(),
        schedule,
    })
}

pub(super) fn journal_for_schedule(
    binding: &BftSignerBindingV2,
    schedule: &BftHeightProposerScheduleV2,
) -> Result<Option<BftSigningJournalV2>, BftSignerErrorV2> {
    let set = schedule.active_set();
    let Some(validator) = set.validator(&binding.local_validator_id) else {
        return Ok(None);
    };
    if !binding.provisioned_signing_keys.iter().any(|key| {
        key.validator_id == validator.validator_id && key.public_key_hex == validator.public_key_hex
    }) {
        return Ok(None);
    }
    let journal = BftSigningJournalV2::for_schedule(
        schedule,
        binding.local_validator_id.clone(),
        validator.public_key_hex.clone(),
    )
    .map_err(|error| invalid(format!("BFT V2 initial signer journal: {error}")))?;
    Ok(Some(journal))
}

#[derive(Debug, Clone)]
struct PendingInstallContextV2 {
    source_height: u64,
    source_parent_commit_ref: String,
    source_block_id: String,
    transition: BftValidatorTransitionV2,
}

#[derive(Debug, Clone)]
pub(super) struct InstallContextV2 {
    pub(super) source_height: u64,
    pub(super) source_parent_commit_ref: String,
    pub(super) transition_block_id: String,
    pub(super) transition_digest: String,
    pub(super) confirmation_height: u64,
    pub(super) confirmation_block_id: String,
    pub(super) activation_height: u64,
    pub(super) old_validator_set_hash: String,
    pub(super) new_validator_set_hash: String,
    pub(super) authority_epoch: u64,
    pub(super) active_set: BftValidatorSetV2,
}

pub(super) fn install_contexts_for_state(
    binding: &BftSignerBindingV2,
    state: &BftSignerStateV2,
) -> Result<Vec<InstallContextV2>, BftSignerErrorV2> {
    let (_, contexts) = reconstruct_install_contexts(binding, &state.history)?;
    Ok(contexts)
}

pub(super) fn validate_signing_install_gate(
    binding: &BftSignerBindingV2,
    state: &BftSignerStateV2,
    schedule: &BftHeightProposerScheduleV2,
) -> Result<(), BftSignerErrorV2> {
    if !local_key_is_active(binding, schedule.active_set()) {
        return Err(invalid(
            "BFT V2 local validator key is not active at this height",
        ));
    }
    let Some(activation_height) =
        (schedule.last_activation_height() > 0).then_some(schedule.last_activation_height())
    else {
        return Ok(());
    };
    let contexts = install_contexts_for_state(binding, state)?;
    let context = contexts
        .iter()
        .find(|context| context.activation_height == activation_height)
        .ok_or_else(|| invalid("BFT V2 active transition has no confirmed install context"))?;
    let receipt = state
        .install_receipts
        .iter()
        .find(|receipt| receipt.body.source_height == context.source_height)
        .ok_or_else(|| {
            invalid("BFT V2 local validator installation is not durably acknowledged")
        })?;
    let active_validator = schedule
        .active_set()
        .validator(&binding.local_validator_id)
        .ok_or_else(|| invalid("BFT V2 active local validator is missing"))?;
    if receipt.body.active_validator_set_hash != schedule.active_set().set_hash()
        || receipt.body.authority_epoch != schedule.authority_epoch()
        || receipt.body.local_signing_public_key != active_validator.public_key_hex
        || receipt.body.activation_height != schedule.last_activation_height()
    {
        return Err(invalid(
            "BFT V2 install receipt does not authorize the current active key and schedule",
        ));
    }
    Ok(())
}

fn reconstruct_install_contexts(
    binding: &BftSignerBindingV2,
    history: &[BftCommitCertificateV2],
) -> Result<(BftHeightProposerScheduleV2, Vec<InstallContextV2>), BftSignerErrorV2> {
    let mut schedule = BftHeightProposerScheduleV2::genesis(
        binding.validate()?,
        binding.schedule_policy.clone(),
        binding.genesis_authority_epoch,
        binding.genesis_state_root.clone(),
        binding.genesis_manifest_hash.clone(),
    )
    .map_err(|error| invalid(format!("BFT V2 install genesis schedule: {error}")))?;
    let mut pending: Option<PendingInstallContextV2> = None;
    let mut contexts = Vec::new();

    for certificate in history {
        let verified = verify_commit_certificate_token_v2(&schedule, certificate)
            .map_err(|error| invalid(format!("BFT V2 install history replay: {error}")))?;
        let block = verified.block();
        let block_id = verified.block_id().to_string();

        if let Some(source) = pending.take() {
            let transition = &source.transition;
            let expected_confirmation_height = source
                .source_height
                .checked_add(1)
                .ok_or_else(|| invalid("BFT V2 install confirmation height overflow"))?;
            let reference = transition
                .committed_reference(source.source_block_id.clone())
                .map_err(|error| {
                    invalid(format!("BFT V2 install transition reference: {error}"))
                })?;
            if block.transition_ref.as_ref() != Some(&reference)
                || block.height != expected_confirmation_height
            {
                return Err(invalid(
                    "BFT V2 install history is missing its exact transition confirmation",
                ));
            }
            let activated = verified.next_height_schedule();
            if activated.height() != transition.activation_height
                || activated.parent_commit_ref() != block_id
                || activated.parent_state_root() != block.state_root
                || activated.parent_manifest_hash() != block.post_manifest_hash
                || activated.active_set().set_hash() != transition.new_validator_set_hash
            {
                return Err(invalid(
                    "BFT V2 install history does not derive the exact activation schedule",
                ));
            }
            contexts.push(InstallContextV2 {
                source_height: source.source_height,
                source_parent_commit_ref: source.source_parent_commit_ref,
                transition_block_id: source.source_block_id,
                transition_digest: transition.transition_digest().map_err(|error| {
                    invalid(format!("BFT V2 install transition digest: {error}"))
                })?,
                confirmation_height: block.height,
                confirmation_block_id: block_id.clone(),
                activation_height: activated.height(),
                old_validator_set_hash: transition.old_validator_set_hash.clone(),
                new_validator_set_hash: transition.new_validator_set_hash.clone(),
                authority_epoch: activated.authority_epoch(),
                active_set: activated.active_set().clone(),
            });
        }

        if let Some(transition) = &block.transition {
            if pending.is_some() {
                return Err(invalid("BFT V2 install history overlaps transitions"));
            }
            pending = Some(PendingInstallContextV2 {
                source_height: block.height,
                source_parent_commit_ref: transition.source_commit_ref.clone(),
                source_block_id: block_id,
                transition: transition.clone(),
            });
        }
        schedule = verified.next_height_schedule().clone();
    }
    Ok((schedule, contexts))
}

fn validate_installed_transitions(
    binding: &BftSignerBindingV2,
    contexts: &[InstallContextV2],
    current_schedule: &BftHeightProposerScheduleV2,
    state: &BftSignerStateV2,
) -> Result<(), BftSignerErrorV2> {
    let mut installed_sources = std::collections::BTreeSet::new();
    let mut previous_revision = 0;
    let mut previous_source_height = 0;
    for receipt in &state.install_receipts {
        let body = &receipt.body;
        let context = contexts
            .iter()
            .find(|context| context.source_height == body.source_height)
            .ok_or_else(|| invalid("BFT V2 install receipt has no committed transition"))?;
        let install_schedule_height = body
            .installed_head_height
            .checked_add(1)
            .ok_or_else(|| invalid("BFT V2 installed head height overflow"))?;
        let applicable_context = contexts
            .iter()
            .filter(|context| context.activation_height <= install_schedule_height)
            .max_by_key(|context| context.activation_height)
            .ok_or_else(|| invalid("BFT V2 installed head has no active transition context"))?;
        let head_index = usize::try_from(body.installed_head_height.saturating_sub(1))
            .map_err(|_| invalid("BFT V2 installed head index overflow"))?;
        let installed_head = state
            .history
            .get(head_index)
            .filter(|certificate| certificate.proposal.block.height == body.installed_head_height)
            .ok_or_else(|| invalid("BFT V2 installed head is absent from committed history"))?;
        let installed_head_block = &installed_head.proposal.block;
        let installed_head_id = installed_head_block
            .block_id()
            .map_err(|error| invalid(format!("BFT V2 installed head block ID: {error}")))?;
        if !installed_sources.insert(body.source_height)
            || body.source_height <= previous_source_height
            || body.installed_revision <= previous_revision
            || body.installed_revision > state.revision
            || !local_key_is_active(binding, &context.active_set)
            || context
                .active_set
                .validator(&binding.local_validator_id)
                .is_none_or(|validator| validator.public_key_hex != body.local_signing_public_key)
            || body.source_height != context.source_height
            || body.transition_source_parent_commit_ref != context.source_parent_commit_ref
            || body.transition_block_id != context.transition_block_id
            || body.transition_digest != context.transition_digest
            || body.confirmation_height != context.confirmation_height
            || body.confirmation_block_id != context.confirmation_block_id
            || body.activation_height != context.activation_height
            || body.installed_head_height < context.confirmation_height
            || body.old_validator_set_hash != context.old_validator_set_hash
            || body.new_validator_set_hash != context.new_validator_set_hash
            || applicable_context.source_height != context.source_height
            || body.active_validator_set_hash != applicable_context.new_validator_set_hash
            || body.authority_epoch != applicable_context.authority_epoch
            || body.runtime_instance_id != binding.operator_instance_id
            || body.installed_head_commit_ref != installed_head_id
            || body.installed_state_root != installed_head_block.state_root
            || body.installed_manifest_hash != installed_head_block.post_manifest_hash
        {
            return Err(invalid(
                "BFT V2 install receipt does not match its committed transition",
            ));
        }
        previous_revision = body.installed_revision;
        previous_source_height = body.source_height;
    }

    if let Some(intent) = &state.pending_install {
        let body = &intent.body;
        let context = contexts
            .iter()
            .find(|context| context.source_height == body.transition_source_height)
            .ok_or_else(|| invalid("BFT V2 install intent has no committed transition"))?;
        let expected_revision = body
            .expected_revision
            .checked_add(1)
            .ok_or_else(|| invalid("BFT V2 install intent revision overflow"))?;
        let tip_height = state
            .history
            .last()
            .map(|certificate| certificate.proposal.block.height);
        let tip = state
            .history
            .last()
            .map(|certificate| &certificate.proposal.block);
        let current_context = contexts
            .iter()
            .find(|context| context.activation_height == current_schedule.last_activation_height());
        if state.revision != expected_revision
            || tip_height != Some(body.install_head_height)
            || tip.is_none_or(|block| {
                block.block_id().ok().as_deref() != Some(body.install_parent_commit_ref.as_str())
                    || block.state_root != body.install_state_root
                    || block.post_manifest_hash != body.install_manifest_hash
            })
            || !local_key_is_active(binding, &context.active_set)
            || context
                .active_set
                .validator(&binding.local_validator_id)
                .is_none_or(|validator| validator.public_key_hex != body.local_signing_public_key)
            || body.transition_source_parent_commit_ref != context.source_parent_commit_ref
            || body.transition_block_id != context.transition_block_id
            || body.transition_digest != context.transition_digest
            || body.confirmation_height != context.confirmation_height
            || body.confirmation_block_id != context.confirmation_block_id
            || body.activation_height != context.activation_height
            || body.old_validator_set_hash != context.old_validator_set_hash
            || body.new_validator_set_hash != context.new_validator_set_hash
            || current_context.is_none_or(|current| current.source_height != context.source_height)
            || body.install_head_height != current_schedule.height().saturating_sub(1)
            || body.install_active_set_hash != current_schedule.active_set().set_hash()
            || body.install_active_set_hash != context.new_validator_set_hash
            || body.target_authority_epoch != current_schedule.authority_epoch()
            || body.target_authority_epoch != context.authority_epoch
            || body.install_parent_commit_ref != current_schedule.parent_commit_ref()
            || body.install_state_root != current_schedule.parent_state_root()
            || body.install_manifest_hash != current_schedule.parent_manifest_hash()
        {
            return Err(invalid(
                "BFT V2 pending install intent does not match its committed transition",
            ));
        }
        let expected_preinstall_digest = digest_hex(
            STATE_DOMAIN_V2,
            &StateDigestInput {
                schema_version: state.schema_version,
                binding_digest: &state.binding_digest,
                revision: body.expected_revision,
                history: &state.history,
                journal: &state.journal,
                pending_install: &None,
                install_receipts: &state.install_receipts,
            },
        )?;
        if body.expected_state_digest != expected_preinstall_digest {
            return Err(invalid(
                "BFT V2 install intent does not preserve its exact preinstall state",
            ));
        }
    }
    Ok(())
}

fn local_key_is_active(binding: &BftSignerBindingV2, set: &BftValidatorSetV2) -> bool {
    set.validator(&binding.local_validator_id)
        .is_some_and(|validator| {
            binding.provisioned_signing_keys.iter().any(|identity| {
                identity.validator_id == validator.validator_id
                    && identity.public_key_hex == validator.public_key_hex
            })
        })
}

fn validate_install_records(
    binding: &BftSignerBindingV2,
    provider_public_key: Option<&str>,
    state: &BftSignerStateV2,
) -> Result<(), BftSignerErrorV2> {
    if state.install_receipts.len() > BFT_SIGNER_MAX_HISTORY_HEIGHTS_V2 {
        return Err(invalid("BFT V2 install receipt count exceeds bound"));
    }
    let bytes = encode_bounded(
        &state.install_receipts,
        BFT_SIGNER_MAX_INSTALL_HISTORY_BYTES_V2,
    )?;
    if bytes.len() > BFT_SIGNER_MAX_INSTALL_HISTORY_BYTES_V2 {
        return Err(invalid("BFT V2 install receipt history byte bound"));
    }
    if (!state.install_receipts.is_empty() || state.pending_install.is_some())
        && provider_public_key.is_none()
    {
        return Err(invalid(
            "BFT V2 install state requires the pinned provider key",
        ));
    }
    for receipt in &state.install_receipts {
        let bytes = encode_bounded(receipt, BFT_SIGNER_MAX_INSTALL_RECORD_BYTES_V2)?;
        if bytes.len() > BFT_SIGNER_MAX_INSTALL_RECORD_BYTES_V2 {
            return Err(invalid("BFT V2 install receipt byte bound"));
        }
        if let Some(provider_public_key) = provider_public_key {
            receipt.verify_for(binding, provider_public_key)?;
        }
    }
    let mut request_ids = std::collections::BTreeSet::new();
    for receipt in &state.install_receipts {
        if !request_ids.insert(receipt.body.request_id.as_str()) {
            return Err(invalid("BFT V2 install receipt request ID is duplicated"));
        }
    }
    if let Some(intent) = &state.pending_install {
        let bytes = encode_bounded(intent, BFT_SIGNER_MAX_INSTALL_RECORD_BYTES_V2)?;
        if bytes.len() > BFT_SIGNER_MAX_INSTALL_RECORD_BYTES_V2 {
            return Err(invalid("BFT V2 pending install intent byte bound"));
        }
        if let Some(provider_public_key) = provider_public_key {
            intent.verify_for(binding, provider_public_key)?;
        }
        if request_ids.contains(intent.body.request_id.as_str()) {
            return Err(invalid(
                "BFT V2 pending install request ID is already completed",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_candidate_transition(
    binding: &BftSignerBindingV2,
    previous: &VerifiedBftSignerStateV2,
    candidate: &BftSignerCandidateV2,
    provider_public_key: &str,
) -> Result<VerifiedBftSignerStateV2, BftSignerErrorV2> {
    validate_history_extension(&previous.state.history, &candidate.history)?;
    validate_candidate_step(binding, previous, candidate)?;
    let revision = previous
        .state
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("BFT V2 signer revision overflow"))?;
    let next = BftSignerStateV2::from_candidate(
        binding,
        revision,
        candidate.history.clone(),
        candidate.journal.clone(),
        candidate.pending_install.clone(),
        candidate.install_receipts.clone(),
        Some(provider_public_key),
    )?;
    validate_state(binding, Some(provider_public_key), &next)
}

fn validate_candidate_step(
    binding: &BftSignerBindingV2,
    previous: &VerifiedBftSignerStateV2,
    candidate: &BftSignerCandidateV2,
) -> Result<(), BftSignerErrorV2> {
    let old = &previous.state;
    let history_added = candidate.history.len() - old.history.len();
    if history_added > 1 {
        return Err(invalid(
            "BFT V2 signer candidate must append at most one verified certificate",
        ));
    }

    if old.pending_install.is_some() {
        validate_pending_install_step(old, candidate)?;
        return Ok(());
    }

    if history_added == 1 {
        if candidate.pending_install.is_some() || candidate.install_receipts != old.install_receipts
        {
            return Err(invalid(
                "BFT V2 certificate append cannot alter local install evidence",
            ));
        }
        let certificate = candidate
            .history
            .last()
            .ok_or_else(|| invalid("BFT V2 certificate append omitted its certificate"))?;
        let token = verify_commit_certificate_token_v2(&previous.schedule, certificate)
            .map_err(|error| invalid(format!("BFT V2 certificate append: {error}")))?;
        let next_schedule = token.next_height_schedule();
        let expected_journal = journal_for_schedule(binding, next_schedule)?;
        if candidate.journal != expected_journal {
            return Err(invalid(
                "BFT V2 certificate append did not rotate to the exact successor journal",
            ));
        }
        if let Some(old_journal) = old.journal.as_ref() {
            if old_journal.pending_commit() != Some(certificate) {
                return Err(invalid(
                    "BFT V2 active signer may append only its exact reserved commit",
                ));
            }
            // Validate the old identity's complete journal and exact pending
            // QC, then derive the successor independently of whether that
            // identity remains active or its key rotates in the next set.
            let derived = verify_reserved_bft_commit_transition_v2(
                old_journal,
                certificate,
                previous.schedule.active_set(),
                &previous.schedule,
                binding.timeouts,
            )
            .map_err(|error| invalid(format!("BFT V2 commit journal transition: {error}")))?;
            if &derived != next_schedule {
                return Err(invalid(
                    "BFT V2 commit journal derived a different successor schedule",
                ));
            }
        }
        return Ok(());
    }

    if candidate.pending_install != old.pending_install
        || candidate.install_receipts != old.install_receipts
    {
        if old.pending_install.is_none()
            && candidate.pending_install.is_some()
            && candidate.install_receipts == old.install_receipts
            && candidate.history == old.history
            && candidate.journal == old.journal
        {
            return Ok(());
        }
        return Err(invalid(
            "BFT V2 install evidence must be prepared or completed as one isolated step",
        ));
    }

    if candidate.history != old.history {
        return Err(invalid(
            "BFT V2 signer history is not an exact one-step append",
        ));
    }
    match (&old.journal, &candidate.journal) {
        (Some(old_journal), Some(next_journal)) => {
            if verify_bft_signing_journal_transition_v2(
                old_journal,
                next_journal,
                previous.schedule.active_set(),
                &previous.schedule,
                binding.timeouts,
            )
            .is_err()
            {
                return Err(invalid("BFT V2 signer journal is not one legal transition"));
            }
        }
        (None, None) => {}
        _ => {
            return Err(invalid(
                "BFT V2 signer journal cannot be created or removed",
            ));
        }
    }
    if candidate.journal == old.journal {
        return Err(invalid(
            "BFT V2 signer candidate does not change durable state",
        ));
    }
    Ok(())
}

fn validate_pending_install_step(
    old: &BftSignerStateV2,
    candidate: &BftSignerCandidateV2,
) -> Result<(), BftSignerErrorV2> {
    if candidate.history != old.history || candidate.journal != old.journal {
        return Err(invalid(
            "BFT V2 pending install must be completed before history or journal changes",
        ));
    }
    match (&old.pending_install, &candidate.pending_install) {
        (Some(intent), Some(next_intent))
            if intent == next_intent && candidate.install_receipts == old.install_receipts =>
        {
            Err(invalid("BFT V2 pending install has no state transition"))
        }
        (Some(intent), None)
            if candidate.install_receipts.len() == old.install_receipts.len() + 1
                && candidate
                    .install_receipts
                    .starts_with(old.install_receipts.as_slice())
                && candidate.install_receipts.last().is_some_and(|receipt| {
                    receipt.body.request_id == intent.body.request_id
                        && intent
                            .digest()
                            .is_ok_and(|digest| receipt.body.intent_digest == digest)
                }) =>
        {
            Ok(())
        }
        _ => Err(invalid(
            "BFT V2 pending install can only complete with its exact receipt",
        )),
    }
}

fn validate_history_extension(
    previous: &[BftCommitCertificateV2],
    next: &[BftCommitCertificateV2],
) -> Result<(), BftSignerErrorV2> {
    if next.len() < previous.len() || next.get(..previous.len()) != Some(previous) {
        return Err(invalid("BFT V2 signer history is not an exact extension"));
    }
    if next.len() > BFT_SIGNER_MAX_HISTORY_HEIGHTS_V2 {
        return Err(invalid("BFT V2 signer history capacity exhausted"));
    }
    Ok(())
}

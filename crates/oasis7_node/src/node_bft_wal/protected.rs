use std::path::Path;

use oasis7_consensus::tendermint_v1::{
    BFT_MAX_SIGNING_JOURNAL_BYTES_V1, BftHeightProposerScheduleV1, BftSigningJournalEventV1,
    BftSigningJournalV1, BftValidatorSetV1, BftVoteIntentV1, complete_reserved_bft_proposal_v1,
    complete_reserved_bft_vote_v1, verify_bft_signing_journal_transition_v1,
    verify_commit_certificate_token_v1,
};
use oasis7_distfs::protected_bft::{
    BftProtectedCheckpointV1, BftSignerBindingV1, BftSignerCandidateV1, BftSignerLeaseV1,
    BftSignerProviderV1, BftSignerStateV1,
};
use oasis7_proto::distributed_bft::{
    BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1, BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1,
    BftCommitCertificateV1, BftProposalV1, BftVoteV1,
};
use serde::{Deserialize, Serialize};

use super::{
    BFT_WAL_ANCHOR_FILE, BFT_WAL_FILE, BFT_WAL_PROTECTED_FILE, BFT_WAL_PROTECTED_MARKER_FILE,
    BftWalStateV1, BftWalStore, acquire_wal_lock, ensure_directory, path_exists_without_following,
    persist_bytes, read_bounded_file_with_limit,
};
use crate::NodeError;

const PROTECTED_MIRROR_SCHEMA_V1: u32 = 1;
const PROTECTED_MODE_SCHEMA_V1: u32 = 1;
const PROTECTED_MARKER_MAX_BYTES: usize = 16 * 1024;
const PROTECTED_MIRROR_MAX_BYTES: usize =
    BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1 + BFT_MAX_SIGNING_JOURNAL_BYTES_V1 + 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtectedModeMarkerV1 {
    schema_version: u32,
    binding_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtectedWalMirrorV1 {
    schema_version: u32,
    binding_digest: String,
    state: BftSignerStateV1,
}

pub(super) struct ProtectedWal {
    lease: Box<dyn BftSignerLeaseV1>,
    binding: BftSignerBindingV1,
    set: BftValidatorSetV1,
    state: BftSignerStateV1,
    poisoned: bool,
}

pub(super) fn open_for_recovery(
    directory: &Path,
    genesis_schedule: &BftHeightProposerScheduleV1,
    binding: &BftSignerBindingV1,
    has_committed_history: bool,
    provider: &dyn BftSignerProviderV1,
    driver_history: &[BftCommitCertificateV1],
) -> Result<BftWalStore, NodeError> {
    ensure_directory(directory)?;
    let lock = acquire_wal_lock(directory)?;
    if genesis_schedule.height() != 1
        || genesis_schedule.parent_commit_ref() != genesis_schedule.scope().genesis_hash
    {
        return Err(consensus_error(
            "protected BFT WAL requires the configured genesis schedule",
        ));
    }

    let set = binding
        .validate()
        .map_err(|error| consensus_error(format!("invalid protected BFT binding: {error}")))?;
    validate_binding_schedule(binding, &set, genesis_schedule)?;
    if has_committed_history == driver_history.is_empty() {
        return Err(consensus_error(
            "protected BFT driver history flag differs from its complete history",
        ));
    }
    validate_history_bounds(driver_history)?;
    let driver_schedule = replay_history(&set, genesis_schedule, driver_history)?;

    reject_local_wal_material(directory)?;
    let binding_digest = binding
        .digest()
        .map_err(|error| consensus_error(format!("protected BFT binding digest: {error}")))?;
    let marker_path = directory.join(BFT_WAL_PROTECTED_MARKER_FILE);
    let mirror_path = directory.join(BFT_WAL_PROTECTED_FILE);
    let marker_exists = path_exists_without_following(&marker_path);
    let mirror_exists = path_exists_without_following(&mirror_path);
    if marker_exists {
        validate_marker(marker_path.as_path(), binding_digest.as_str())?;
    } else if mirror_exists {
        return Err(consensus_error(
            "protected BFT mirror has no mode marker; preserve evidence and stop",
        ));
    }

    // Do not acquire the remote signer lease until this process owns the local
    // WAL lock and has rejected incompatible local/mode evidence. In
    // particular, a second startup against the same directory must not fence
    // the live process's protected signer before failing the local lock.
    let mut lease = provider
        .acquire(binding)
        .map_err(|error| consensus_error(format!("acquire protected BFT signer: {error}")))?;
    let checkpoint = lease
        .checkpoint()
        .map_err(|error| consensus_error(format!("read protected BFT checkpoint: {error}")))?;
    validate_checkpoint(&checkpoint, binding, &set)?;
    validate_driver_history(&checkpoint.state, driver_history, &driver_schedule)?;

    if mirror_exists {
        let mirror = read_mirror(mirror_path.as_path(), binding_digest.as_str())?;
        mirror
            .state
            .validate(binding)
            .map_err(|error| consensus_error(format!("invalid protected BFT mirror: {error}")))?;
        if mirror.state.revision > checkpoint.state.revision {
            return Err(consensus_error(
                "local protected BFT mirror is ahead of the signer checkpoint; stop",
            ));
        }
        if mirror.state.revision == checkpoint.state.revision
            && (mirror.state.state_digest != checkpoint.state.state_digest
                || mirror.state != checkpoint.state)
        {
            return Err(consensus_error(
                "local protected BFT mirror conflicts at the signer revision; stop",
            ));
        }
    }

    let mut store = BftWalStore {
        directory: directory.to_path_buf(),
        state: checkpoint.state.journal.clone(),
        local_validator_id: binding.local_validator_id.clone(),
        local_signing_public_key: binding.local_signing_public_key.clone(),
        protected: Some(ProtectedWal {
            lease,
            binding: binding.clone(),
            set,
            state: checkpoint.state,
            poisoned: false,
        }),
        _lock: lock,
    };

    if !marker_exists {
        persist_marker(directory, binding_digest.as_str())?;
    }
    persist_mirror_from_store(&mut store)?;
    Ok(store)
}

pub(super) fn protected_time_ms(store: &mut BftWalStore) -> Result<Option<i64>, NodeError> {
    let Some(protected) = store.protected.as_mut() else {
        return Ok(None);
    };
    ensure_healthy(protected)?;
    let time_ms = match protected.lease.protected_time_ms() {
        Ok(time_ms) => time_ms,
        Err(error) => return Err(poison(protected, format!("protected BFT clock: {error}"))),
    };
    if time_ms < 0
        || protected
            .state
            .journal
            .events()
            .last()
            .map(BftSigningJournalEventV1::event_time_ms)
            .or(protected.state.journal.height_started_at_ms)
            .is_some_and(|floor| time_ms < floor)
    {
        return Err(poison(
            protected,
            "protected BFT clock regressed below the journal floor",
        ));
    }
    Ok(Some(time_ms))
}

pub(super) fn start_height_timer(store: &mut BftWalStore) -> Result<i64, NodeError> {
    let Some(protected) = store.protected.as_mut() else {
        return Err(consensus_error("protected BFT signer is not configured"));
    };
    ensure_healthy(protected)?;
    if let Some(started_at_ms) = protected.state.journal.height_started_at_ms {
        return Ok(started_at_ms);
    }
    let checkpoint = match protected.lease.start_height_timer(protected.state.revision) {
        Ok(checkpoint) => checkpoint,
        Err(error) => {
            return Err(poison(
                protected,
                format!("start protected BFT timer: {error}"),
            ));
        }
    };
    let started_at_ms = checkpoint.state.journal.height_started_at_ms;
    let Some(started_at_ms) = started_at_ms else {
        return Err(poison(
            protected,
            "protected BFT timer response omitted its start time",
        ));
    };
    let mut expected = protected.state.journal.clone();
    expected.height_started_at_ms = Some(started_at_ms);
    let history = protected.state.history.clone();
    let directory = store.directory.clone();
    accept_checkpoint_transition(
        protected,
        directory.as_path(),
        checkpoint,
        history,
        expected,
    )?;
    store.state = protected.state.journal.clone();
    Ok(started_at_ms)
}

pub(super) fn persist_journal(
    store: &mut BftWalStore,
    next: BftWalStateV1,
) -> Result<(), NodeError> {
    let Some(protected) = store.protected.as_mut() else {
        return Err(consensus_error("protected BFT signer is not configured"));
    };
    ensure_healthy(protected)?;
    let old_journal = protected.state.journal.clone();
    if old_journal == next {
        return Ok(());
    }
    let schedule = replay_history(
        &protected.set,
        &genesis_schedule(protected)?,
        &protected.state.history,
    )?;
    let next_schedule = verify_bft_signing_journal_transition_v1(
        &old_journal,
        &next,
        &protected.set,
        &schedule,
        protected.binding.timeouts,
    )
    .map_err(|error| consensus_error(format!("invalid protected BFT WAL transition: {error}")))?;
    let next_history = match next.height.cmp(&old_journal.height) {
        std::cmp::Ordering::Equal => protected.state.history.clone(),
        std::cmp::Ordering::Greater => {
            let mut history = protected.state.history.clone();
            let certificate = old_journal.pending_commit().ok_or_else(|| {
                consensus_error("protected BFT height advance lacks its pending QC")
            })?;
            if next.height != old_journal.height.saturating_add(1)
                || next
                    != BftSigningJournalV1::for_schedule(
                        &next_schedule,
                        protected.binding.local_validator_id.clone(),
                        protected.binding.local_signing_public_key.clone(),
                    )
            {
                return Err(consensus_error(
                    "protected BFT height advance is not the exact empty QC successor",
                ));
            }
            history.push(certificate.clone());
            history
        }
        std::cmp::Ordering::Less => {
            return Err(consensus_error("protected BFT journal height regressed"));
        }
    };
    let candidate = BftSignerCandidateV1 {
        history: next_history.clone(),
        journal: next.clone(),
    };
    let checkpoint = match protected.lease.advance(
        protected.state.revision,
        protected.state.state_digest.as_str(),
        candidate,
    ) {
        Ok(checkpoint) => checkpoint,
        Err(error) => {
            return Err(poison(
                protected,
                format!("advance protected BFT WAL: {error}"),
            ));
        }
    };
    let directory = store.directory.clone();
    accept_checkpoint_transition(
        protected,
        directory.as_path(),
        checkpoint,
        next_history,
        next,
    )?;
    store.state = protected.state.journal.clone();
    Ok(())
}

pub(super) fn sign_reserved_proposal(
    store: &mut BftWalStore,
    intent: &oasis7_consensus::tendermint_v1::BftProposalIntentV1,
) -> Result<Option<(BftProposalV1, i64)>, NodeError> {
    let Some(protected) = store.protected.as_mut() else {
        return Ok(None);
    };
    ensure_healthy(protected)?;
    if let Some((proposal, at)) = completed_proposal(protected, intent) {
        return Ok(Some((proposal, at)));
    }
    if protected.state.journal.pending_proposal_intent() != Some(intent) {
        return Err(consensus_error(
            "protected BFT proposal signing requires the exact reserved intent",
        ));
    }
    let schedule = replay_history(
        &protected.set,
        &genesis_schedule(protected)?,
        &protected.state.history,
    )?;
    let (proposal, checkpoint) = match protected
        .lease
        .sign_reserved_proposal(protected.state.revision, intent)
    {
        Ok(result) => result,
        Err(error) => {
            return Err(poison(
                protected,
                format!("protected BFT proposal signing: {error}"),
            ));
        }
    };
    let now_ms = match exact_proposal_checkpoint_time(&checkpoint, &proposal) {
        Ok(now_ms) => now_ms,
        Err(error) => return Err(poison(protected, error.to_string())),
    };
    let expected = match complete_reserved_bft_proposal_v1(
        &protected.state.journal,
        proposal.clone(),
        now_ms,
        &protected.set,
        &schedule,
        protected.binding.timeouts,
    ) {
        Ok(expected) => expected,
        Err(error) => {
            return Err(poison(
                protected,
                format!("invalid protected BFT proposal decision: {error}"),
            ));
        }
    };
    let history = protected.state.history.clone();
    let directory = store.directory.clone();
    accept_checkpoint_transition(
        protected,
        directory.as_path(),
        checkpoint,
        history,
        expected,
    )?;
    store.state = protected.state.journal.clone();
    Ok(Some((proposal, now_ms)))
}

pub(super) fn sign_reserved_vote(
    store: &mut BftWalStore,
    intent: &BftVoteIntentV1,
) -> Result<Option<(BftVoteV1, i64)>, NodeError> {
    let Some(protected) = store.protected.as_mut() else {
        return Ok(None);
    };
    ensure_healthy(protected)?;
    if let Some((vote, at)) = completed_vote(protected, intent) {
        return Ok(Some((vote, at)));
    }
    if protected.state.journal.pending_vote_intent() != Some(intent) {
        return Err(consensus_error(
            "protected BFT vote signing requires the exact reserved intent",
        ));
    }
    let schedule = replay_history(
        &protected.set,
        &genesis_schedule(protected)?,
        &protected.state.history,
    )?;
    let (vote, checkpoint) = match protected
        .lease
        .sign_reserved_vote(protected.state.revision, intent)
    {
        Ok(result) => result,
        Err(error) => {
            return Err(poison(
                protected,
                format!("protected BFT vote signing: {error}"),
            ));
        }
    };
    let now_ms = match exact_vote_checkpoint_time(&checkpoint, &vote) {
        Ok(now_ms) => now_ms,
        Err(error) => return Err(poison(protected, error.to_string())),
    };
    let expected = match complete_reserved_bft_vote_v1(
        &protected.state.journal,
        vote.clone(),
        now_ms,
        &protected.set,
        &schedule,
        protected.binding.timeouts,
    ) {
        Ok(expected) => expected,
        Err(error) => {
            return Err(poison(
                protected,
                format!("invalid protected BFT vote decision: {error}"),
            ));
        }
    };
    let history = protected.state.history.clone();
    let directory = store.directory.clone();
    accept_checkpoint_transition(
        protected,
        directory.as_path(),
        checkpoint,
        history,
        expected,
    )?;
    store.state = protected.state.journal.clone();
    Ok(Some((vote, now_ms)))
}

pub(super) fn accept_signed_proposal(
    store: &mut BftWalStore,
    intent: &oasis7_consensus::tendermint_v1::BftProposalIntentV1,
    proposal: BftProposalV1,
    now_ms: i64,
) -> Result<(), NodeError> {
    let Some(protected) = store.protected.as_ref() else {
        return Err(consensus_error("protected BFT signer is not configured"));
    };
    ensure_healthy(protected)?;
    if completed_proposal(protected, intent)
        .is_some_and(|(saved, at)| saved == proposal && at == now_ms)
    {
        Ok(())
    } else {
        Err(consensus_error(
            "protected BFT proposal must be durably signed by the configured signer",
        ))
    }
}

pub(super) fn accept_signed_vote(
    store: &mut BftWalStore,
    intent: &BftVoteIntentV1,
    vote: BftVoteV1,
    now_ms: i64,
) -> Result<(), NodeError> {
    let Some(protected) = store.protected.as_ref() else {
        return Err(consensus_error("protected BFT signer is not configured"));
    };
    ensure_healthy(protected)?;
    if completed_vote(protected, intent).is_some_and(|(saved, at)| saved == vote && at == now_ms) {
        Ok(())
    } else {
        Err(consensus_error(
            "protected BFT vote must be durably signed by the configured signer",
        ))
    }
}

fn completed_proposal(
    protected: &ProtectedWal,
    intent: &oasis7_consensus::tendermint_v1::BftProposalIntentV1,
) -> Option<(BftProposalV1, i64)> {
    protected
        .state
        .journal
        .events()
        .iter()
        .find_map(|event| match event {
            BftSigningJournalEventV1::Proposal {
                proposal,
                local: true,
                now_ms,
            } if proposal_matches_intent(proposal.as_ref(), intent) => {
                Some((proposal.as_ref().clone(), *now_ms))
            }
            _ => None,
        })
}

fn completed_vote(protected: &ProtectedWal, intent: &BftVoteIntentV1) -> Option<(BftVoteV1, i64)> {
    protected
        .state
        .journal
        .events()
        .iter()
        .find_map(|event| match event {
            BftSigningJournalEventV1::Vote {
                vote,
                local: true,
                now_ms,
            } if vote_matches_intent(vote.as_ref(), intent) => {
                Some((vote.as_ref().clone(), *now_ms))
            }
            _ => None,
        })
}

fn exact_proposal_checkpoint_time(
    checkpoint: &BftProtectedCheckpointV1,
    proposal: &BftProposalV1,
) -> Result<i64, NodeError> {
    match checkpoint.state.journal.events().last() {
        Some(BftSigningJournalEventV1::Proposal {
            proposal: saved,
            local: true,
            now_ms,
        }) if saved.as_ref() == proposal => Ok(*now_ms),
        _ => Err(consensus_error(
            "protected signer response lacks the exact local proposal journal event",
        )),
    }
}

fn exact_vote_checkpoint_time(
    checkpoint: &BftProtectedCheckpointV1,
    vote: &BftVoteV1,
) -> Result<i64, NodeError> {
    match checkpoint.state.journal.events().last() {
        Some(BftSigningJournalEventV1::Vote {
            vote: saved,
            local: true,
            now_ms,
        }) if saved.as_ref() == vote => Ok(*now_ms),
        _ => Err(consensus_error(
            "protected signer response lacks the exact local vote journal event",
        )),
    }
}

fn accept_checkpoint_transition(
    protected: &mut ProtectedWal,
    directory: &Path,
    checkpoint: BftProtectedCheckpointV1,
    expected_history: Vec<BftCommitCertificateV1>,
    expected_journal: BftSigningJournalV1,
) -> Result<(), NodeError> {
    if let Err(error) = validate_checkpoint(&checkpoint, &protected.binding, &protected.set) {
        return Err(poison(protected, error.to_string()));
    }
    let expected_revision = match protected.state.revision.checked_add(1) {
        Some(revision) => revision,
        None => return Err(poison(protected, "protected BFT revision overflow")),
    };
    if checkpoint.state.revision != expected_revision
        || checkpoint.state.history != expected_history
        || checkpoint.state.journal != expected_journal
    {
        return Err(poison(
            protected,
            "protected signer returned a checkpoint different from the exact requested transition",
        ));
    }
    if let Err(error) = persist_mirror(directory, &protected.binding, &checkpoint.state) {
        return Err(poison(protected, error.to_string()));
    }
    protected.state = checkpoint.state;
    Ok(())
}

fn persist_mirror_from_store(store: &mut BftWalStore) -> Result<(), NodeError> {
    let directory = store.directory.clone();
    let protected = store
        .protected
        .as_mut()
        .ok_or_else(|| consensus_error("protected BFT signer is not configured"))?;
    persist_mirror(directory.as_path(), &protected.binding, &protected.state)?;
    Ok(())
}

fn persist_mirror(
    directory: &Path,
    binding: &BftSignerBindingV1,
    state: &BftSignerStateV1,
) -> Result<(), NodeError> {
    state
        .validate(binding)
        .map_err(|error| consensus_error(format!("validate protected BFT mirror: {error}")))?;
    let binding_digest = binding
        .digest()
        .map_err(|error| consensus_error(format!("protected BFT binding digest: {error}")))?;
    let mirror = ProtectedWalMirrorV1 {
        schema_version: PROTECTED_MIRROR_SCHEMA_V1,
        binding_digest: binding_digest.clone(),
        state: state.clone(),
    };
    let bytes = serde_cbor::to_vec(&mirror)
        .map_err(|error| consensus_error(format!("encode protected BFT mirror: {error}")))?;
    if bytes.len() > PROTECTED_MIRROR_MAX_BYTES {
        return Err(consensus_error(
            "protected BFT local mirror exceeds bounded storage capacity",
        ));
    }
    // The mirror is not signer authority: the authenticated remote checkpoint
    // is validated before every install and on reopen. A stale temporary mirror
    // can therefore be replaced after the remote transition is known.
    persist_bytes(directory, BFT_WAL_PROTECTED_FILE, bytes.as_slice())
}

fn persist_marker(directory: &Path, binding_digest: &str) -> Result<(), NodeError> {
    let marker = ProtectedModeMarkerV1 {
        schema_version: PROTECTED_MODE_SCHEMA_V1,
        binding_digest: binding_digest.to_string(),
    };
    let bytes = serde_cbor::to_vec(&marker)
        .map_err(|error| consensus_error(format!("encode protected BFT mode marker: {error}")))?;
    if bytes.len() > PROTECTED_MARKER_MAX_BYTES {
        return Err(consensus_error(
            "protected BFT mode marker exceeds its bound",
        ));
    }
    persist_bytes(directory, BFT_WAL_PROTECTED_MARKER_FILE, bytes.as_slice())
}

fn validate_marker(path: &Path, binding_digest: &str) -> Result<(), NodeError> {
    let bytes = read_bounded_file_with_limit(
        path,
        "protected BFT mode marker",
        PROTECTED_MARKER_MAX_BYTES,
    )?;
    let marker: ProtectedModeMarkerV1 =
        serde_cbor::from_slice(bytes.as_slice()).map_err(|error| {
            consensus_error(format!(
                "decode protected BFT mode marker; preserve evidence and stop: {error}"
            ))
        })?;
    if serde_cbor::to_vec(&marker).map_err(|error| consensus_error(error.to_string()))? != bytes {
        return Err(consensus_error(
            "protected BFT mode marker is not canonical CBOR; preserve evidence and stop",
        ));
    }
    if marker.schema_version != PROTECTED_MODE_SCHEMA_V1 || marker.binding_digest != binding_digest
    {
        return Err(consensus_error(
            "protected BFT mode marker differs from configured signer binding",
        ));
    }
    Ok(())
}

fn read_mirror(path: &Path, binding_digest: &str) -> Result<ProtectedWalMirrorV1, NodeError> {
    let bytes =
        read_bounded_file_with_limit(path, "protected BFT mirror", PROTECTED_MIRROR_MAX_BYTES)?;
    let mirror: ProtectedWalMirrorV1 =
        serde_cbor::from_slice(bytes.as_slice()).map_err(|error| {
            consensus_error(format!(
                "decode protected BFT mirror; preserve evidence and stop: {error}"
            ))
        })?;
    if serde_cbor::to_vec(&mirror).map_err(|error| consensus_error(error.to_string()))? != bytes {
        return Err(consensus_error(
            "protected BFT mirror is not canonical CBOR; preserve evidence and stop",
        ));
    }
    if mirror.schema_version != PROTECTED_MIRROR_SCHEMA_V1
        || mirror.binding_digest != binding_digest
    {
        return Err(consensus_error(
            "protected BFT mirror schema or binding mismatch",
        ));
    }
    Ok(mirror)
}

fn validate_checkpoint(
    checkpoint: &BftProtectedCheckpointV1,
    binding: &BftSignerBindingV1,
    set: &BftValidatorSetV1,
) -> Result<(), NodeError> {
    checkpoint
        .state
        .validate(binding)
        .map_err(|error| consensus_error(format!("invalid protected BFT checkpoint: {error}")))?;
    let checkpoint_set = binding
        .validate()
        .map_err(|error| consensus_error(format!("invalid protected BFT binding: {error}")))?;
    if &checkpoint_set != set {
        return Err(consensus_error(
            "protected BFT checkpoint validator set differs from configured set",
        ));
    }
    Ok(())
}

fn validate_binding_schedule(
    binding: &BftSignerBindingV1,
    set: &BftValidatorSetV1,
    schedule: &BftHeightProposerScheduleV1,
) -> Result<(), NodeError> {
    if schedule.scope() != &binding.scope
        || schedule.authority_epoch() != binding.authority_epoch
        || schedule.validator_set_hash() != set.set_hash()
        || binding.timeouts.validate().is_err()
    {
        return Err(consensus_error(
            "protected BFT signer binding differs from configured genesis schedule",
        ));
    }
    Ok(())
}

fn validate_driver_history(
    provider: &BftSignerStateV1,
    driver_history: &[BftCommitCertificateV1],
    driver_schedule: &BftHeightProposerScheduleV1,
) -> Result<(), NodeError> {
    let common = provider.history.len().min(driver_history.len());
    if provider.history[..common] != driver_history[..common]
        || provider.history.len() > driver_history.len()
    {
        return Err(consensus_error(
            "protected signer history differs from complete driver history",
        ));
    }
    if provider.history.len() == driver_history.len() {
        if provider.journal.height != driver_schedule.height()
            || provider.journal.parent_commit_ref != driver_schedule.parent_commit_ref()
            || provider.journal.starting_priority
                != *driver_schedule.starting_priority().priorities()
        {
            return Err(consensus_error(
                "protected signer journal differs from complete driver history tip",
            ));
        }
        return Ok(());
    }
    let Some(missing) = driver_history.last() else {
        return Err(consensus_error(
            "protected signer history is ahead of driver history",
        ));
    };
    if driver_history.len() != provider.history.len() + 1
        || provider.journal.pending_commit() != Some(missing)
        || missing.proposal.block.height != provider.journal.height
        || missing.proposal.block.parent_commit_ref != provider.journal.parent_commit_ref
        || driver_schedule.height() != provider.journal.height.saturating_add(1)
    {
        return Err(consensus_error(
            "protected signer is ahead of driver history without the exact pending committed QC",
        ));
    }
    Ok(())
}

fn replay_history(
    set: &BftValidatorSetV1,
    genesis: &BftHeightProposerScheduleV1,
    history: &[BftCommitCertificateV1],
) -> Result<BftHeightProposerScheduleV1, NodeError> {
    validate_history_bounds(history)?;
    let mut schedule = genesis.clone();
    for certificate in history {
        let token = verify_commit_certificate_token_v1(set, &schedule, certificate)
            .map_err(|error| consensus_error(format!("invalid protected BFT history: {error}")))?;
        schedule = token.next_height_schedule().clone();
    }
    Ok(schedule)
}

fn validate_history_bounds(history: &[BftCommitCertificateV1]) -> Result<(), NodeError> {
    if history.len() > BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 {
        return Err(consensus_error(
            "protected BFT history exceeds height capacity",
        ));
    }
    let bytes = serde_cbor::to_vec(&history)
        .map_err(|error| consensus_error(format!("encode protected BFT history: {error}")))?;
    if bytes.len() > BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1 {
        return Err(consensus_error(
            "protected BFT history exceeds byte capacity",
        ));
    }
    Ok(())
}

fn genesis_schedule(protected: &ProtectedWal) -> Result<BftHeightProposerScheduleV1, NodeError> {
    BftHeightProposerScheduleV1::genesis(&protected.set, protected.binding.authority_epoch)
        .map_err(|error| consensus_error(format!("protected BFT genesis schedule: {error}")))
}

fn reject_local_wal_material(directory: &Path) -> Result<(), NodeError> {
    if path_exists_without_following(directory.join(BFT_WAL_FILE).as_path())
        || path_exists_without_following(directory.join(BFT_WAL_ANCHOR_FILE).as_path())
    {
        return Err(consensus_error(
            "local BFT WAL material exists; refusing implicit local-to-protected signer migration",
        ));
    }
    Ok(())
}

fn ensure_healthy(protected: &ProtectedWal) -> Result<(), NodeError> {
    if protected.poisoned {
        return Err(consensus_error(
            "protected BFT WAL is poisoned after an uncertain signer operation; reopen and reconcile",
        ));
    }
    Ok(())
}

fn poison(protected: &mut ProtectedWal, reason: impl Into<String>) -> NodeError {
    protected.poisoned = true;
    consensus_error(format!(
        "{}; protected BFT WAL poisoned, preserve evidence and reopen",
        reason.into()
    ))
}

fn proposal_matches_intent(
    proposal: &BftProposalV1,
    intent: &oasis7_consensus::tendermint_v1::BftProposalIntentV1,
) -> bool {
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

fn consensus_error(reason: impl Into<String>) -> NodeError {
    NodeError::Consensus {
        reason: reason.into(),
    }
}

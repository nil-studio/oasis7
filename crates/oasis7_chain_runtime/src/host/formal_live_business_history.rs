//! Permanent result lookup with current-root delegation of the original cut.
use super::*;
use oasis7::world_service::formal_read::FormalReadHistoryPageV1;
use oasis7::world_service::formal_result::{
    FormalBusinessResultPublication, FormalBusinessResultTreeV1,
    FormalHistoricalBusinessPublicationV1,
};
use oasis7_client_api::world_service::*;
use oasis7_distfs::controlled_authority::formal::{
    FormalRecordKindV1, VerifiedHistoricalCutInclusionV2,
    business_index::{BusinessIndexNodeV1, BusinessIndexNodeWriteV1},
    business_material::FormalBusinessResultEnvelopeV1,
    verify_frozen_material, verify_record_material,
};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
};

const MAX_VERIFIED_BUSINESS_SEED_BYTES: usize = 64 * 1024 * 1024;

#[derive(PartialEq, Eq)]
struct PrewarmAnchorBinding {
    checkpoint: FormalReadCheckpointV1,
    // Checkpoints deliberately do not serialize every opaque authority field,
    // including the healthy durable-set activation digest.
    authority_context: String,
}

impl PrewarmAnchorBinding {
    fn new(anchor: &VerifiedFormalReadAnchorV1) -> Result<Self, String> {
        Ok(Self {
            checkpoint: anchor.checkpoint(),
            authority_context: anchor
                .authority()
                .context_digest()
                .map_err(|error| error.to_string())?,
        })
    }
}

/// A single-use byte-cache receipt, never a persisted checkpoint or a read
/// capability. Only a complete C0 replay followed by successful staged-cache
/// installation constructs this value. The scope retains the exact immutable
/// resolver configuration, and the Arc retains the cache that received bytes.
pub(crate) struct VerifiedBusinessPrewarmReceipt {
    scope: Arc<()>,
    reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    external_origin: PrewarmAnchorBinding,
    trusted: PrewarmAnchorBinding,
    active_context: String,
    target: PrewarmAnchorBinding,
}

impl VerifiedBusinessPrewarmReceipt {
    fn from_completed(
        resolver: &NativeHistoryResolver,
        target: &VerifiedFormalReadAnchorV1,
        reader: &dyn BusinessIndexNodeReader,
    ) -> Result<Option<Self>, String> {
        if !std::ptr::addr_eq(resolver.business_reader.as_ref(), reader) {
            return Ok(None);
        }
        Ok(Some(Self {
            scope: Arc::clone(&resolver.prewarm_scope),
            reader: Arc::clone(&resolver.business_reader),
            external_origin: PrewarmAnchorBinding::new(&resolver.external_origin)?,
            trusted: PrewarmAnchorBinding::new(&resolver.trusted)?,
            active_context: resolver
                .active
                .context_digest()
                .map_err(|error| error.to_string())?,
            target: PrewarmAnchorBinding::new(target)?,
        }))
    }

    fn matches(
        &self,
        resolver: &NativeHistoryResolver,
        target: &VerifiedFormalReadAnchorV1,
        reader: &dyn BusinessIndexNodeReader,
    ) -> Result<bool, String> {
        Ok(Arc::ptr_eq(&self.scope, &resolver.prewarm_scope)
            && std::ptr::addr_eq(self.reader.as_ref(), reader)
            && std::ptr::addr_eq(resolver.business_reader.as_ref(), reader)
            && self.external_origin == PrewarmAnchorBinding::new(&resolver.external_origin)?
            && self.trusted == PrewarmAnchorBinding::new(&resolver.trusted)?
            && self.active_context
                == resolver
                    .active
                    .context_digest()
                    .map_err(|error| error.to_string())?
            && self.target == PrewarmAnchorBinding::new(target)?)
    }
}

#[cfg(test)]
struct LookupTiming {
    enabled: bool,
    started: std::time::Instant,
}

#[cfg(test)]
impl LookupTiming {
    fn new() -> Self {
        let enabled = std::env::var_os("OASIS7_FORMAL_LOOKUP_TIMING").is_some();
        let timing = Self {
            enabled,
            started: std::time::Instant::now(),
        };
        if enabled {
            eprintln!("formal_lookup_timing stage=lookup event=start elapsed_ms=0");
        }
        timing
    }

    fn enter(&self, stage: &'static str) -> Option<std::time::Instant> {
        if self.enabled {
            eprintln!(
                "formal_lookup_timing stage={stage} event=begin elapsed_ms={}",
                self.started.elapsed().as_millis()
            );
            Some(std::time::Instant::now())
        } else {
            None
        }
    }

    fn finish(&self, stage: &'static str, started: Option<std::time::Instant>) {
        if let Some(started) = started {
            eprintln!(
                "formal_lookup_timing stage={stage} event=end stage_ms={} elapsed_ms={}",
                started.elapsed().as_millis(),
                self.started.elapsed().as_millis()
            );
        }
    }
}

impl BusinessIndexNodeReader for NativeHistoryResolver {
    fn prefetch_path(
        &self,
        root: &oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexRootV1,
        key: &str,
    ) -> Result<(), String> {
        self.business_reader.prefetch_path(root, key)
    }

    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.business_reader.read_cached_node(digest)
    }

    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        self.business_reader.cache_content_node(digest, bytes)
    }

    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.business_reader.read_node(digest)
    }
}

fn tree(material: &ClosedRecord) -> Result<FormalBusinessResultTreeV1, String> {
    let envelope: FormalBusinessResultEnvelopeV1 = decode_role(material, ArtifactRole::Result)?;
    FormalBusinessResultTreeV1::decode_cbor(&envelope.tree_descriptor_cbor)
}

/// Replay local publications from the configured external pin and seed only
/// the bounded content-addressed suffix whose proofs advanced opaque anchors.
/// Missing local history merely skips this cache optimization; malformed local
/// material aborts construction before any staged bytes are installed.
pub(super) fn prewarm_qualified_history(
    resolver: &NativeHistoryResolver,
    target: &VerifiedFormalReadAnchorV1,
    reader: &dyn BusinessIndexNodeReader,
) -> Result<(), String> {
    let target_ref = FormalReadAnchorRefV1 {
        position: target.head().record().position,
        record_id: target.head().head().record_id.clone(),
        active_authority_id: target
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?,
    };
    prewarm_history_from_origin(resolver, &target_ref, Some(target), false, reader).map(|_| ())
}

pub(super) fn prewarm_qualified_history_after_hint(
    resolver: &NativeHistoryResolver,
    target: &VerifiedFormalReadAnchorV1,
    reader: &dyn BusinessIndexNodeReader,
    receipt: Option<VerifiedBusinessPrewarmReceipt>,
) -> Result<bool, String> {
    if let Some(receipt) = receipt
        && receipt.matches(resolver, target, reader)?
    {
        return Ok(true);
    }
    prewarm_qualified_history(resolver, target, reader)?;
    Ok(false)
}

/// Use matching fresh protected replica heads only to locate a possible cache
/// suffix. The only authority is still a replay from the configured external
/// C0 anchor. A missing/mismatching target skips this optional optimization;
/// invalid proof or local closure bytes remain fatal.
pub(super) fn prewarm_qualified_history_hint(
    resolver: &NativeHistoryResolver,
    hint: &FormalReadAnchorRefV1,
    reader: &dyn BusinessIndexNodeReader,
) -> Result<Option<VerifiedBusinessPrewarmReceipt>, String> {
    if resolver.active.definition().profile_version != 2
        || hint.position < resolver.external_origin.head().record().position
    {
        return Ok(None);
    }
    match prewarm_history_from_origin(resolver, hint, None, true, reader)? {
        Some(target) => VerifiedBusinessPrewarmReceipt::from_completed(resolver, &target, reader),
        None => Ok(None),
    }
}

fn prewarm_history_from_origin(
    resolver: &NativeHistoryResolver,
    target_ref: &FormalReadAnchorRefV1,
    exact_target: Option<&VerifiedFormalReadAnchorV1>,
    is_head_hint: bool,
    reader: &dyn BusinessIndexNodeReader,
) -> Result<Option<VerifiedFormalReadAnchorV1>, String> {
    if resolver
        .external_origin
        .authority()
        .definition()
        .profile_version
        != 2
    {
        return Ok(None);
    }
    let deadline = std::time::Instant::now()
        .checked_add(resolver.timeout)
        .ok_or("HistoryUnavailable: prewarm deadline overflow")?;
    let mut staged = StartupBusinessNodeStage::default();
    let mut proof_pages =
        open_optional_origin_proof_pages(resolver.origin_history_directory.as_deref())?;
    let replayed = resolver.replay_exact_origin_for_prewarm(
        target_ref,
        if is_head_hint {
            super::ReplayTargetPolicy::ProtectedHeadHint
        } else {
            super::ReplayTargetPolicy::Exact
        },
        deadline,
        |_, position, _, _| {
            if let Some((candidate, proof)) =
                super::super::execution_bridge::formal_writer::read_published_candidate_at(
                    &resolver.writer_directory,
                    position,
                )
                .map_err(super::ReplayFailure::Invalid)?
            {
                return Ok(super::ReplayStepProof {
                    proof,
                    candidate: Some(candidate),
                });
            }
            let Some(proof) = proof_pages
                .as_mut()
                .map(|pages| pages.proof_at(position))
                .transpose()?
                .flatten()
            else {
                return Err(super::ReplayFailure::LocalPublicationMissing);
            };
            Ok(super::ReplayStepProof {
                proof,
                candidate: None,
            })
        },
        |predecessor, anchor, proof, candidate| {
            if let Some(candidate) = candidate {
                stage_verified_publication(&mut staged, predecessor, anchor, proof, candidate)
                    .map_err(super::ReplayFailure::Invalid)?;
            }
            Ok(())
        },
    );
    finish_prewarm_result(
        replayed,
        staged,
        exact_target.map(VerifiedFormalReadAnchorV1::checkpoint),
        is_head_hint,
        |anchor| replayed_context_matches_hint(resolver, anchor, target_ref),
        deadline,
        reader,
    )
}

fn open_optional_origin_proof_pages(
    directory: Option<&Path>,
) -> Result<Option<OriginHistoryProofPages>, String> {
    let Some(directory) = directory else {
        return Ok(None);
    };
    match std::fs::symlink_metadata(directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "HistoryUnavailable: origin proof directory metadata: {error}"
        )),
        Ok(_) => OriginHistoryProofPages::open(directory).map(Some),
    }
}

fn finish_prewarm_result(
    replayed: Result<super::ReplayedHistoryCut, super::ReplayFailure>,
    staged: StartupBusinessNodeStage,
    exact_checkpoint: Option<oasis7_distfs::controlled_authority::formal::FormalReadCheckpointV1>,
    is_head_hint: bool,
    context_matches_hint: impl FnOnce(&VerifiedFormalReadAnchorV1) -> Result<bool, String>,
    deadline: std::time::Instant,
    reader: &dyn BusinessIndexNodeReader,
) -> Result<Option<VerifiedFormalReadAnchorV1>, String> {
    let replayed = match replayed {
        Ok(replayed) => replayed,
        Err(
            super::ReplayFailure::LocalPublicationMissing | super::ReplayFailure::DeadlineExceeded,
        ) => return Ok(None),
        Err(super::ReplayFailure::TargetMismatch) if is_head_hint => return Ok(None),
        Err(super::ReplayFailure::UnqualifiedPendingRecoveryTarget(expected)) => {
            if is_head_hint || exact_checkpoint.as_ref() == Some(expected.as_ref()) {
                return Ok(None);
            }
            return Err(
                "HistoryUnavailable: pending recovery context differs from exact writer anchor"
                    .into(),
            );
        }
        Err(super::ReplayFailure::Invalid(error)) => return Err(error),
        Err(super::ReplayFailure::TargetMismatch) => {
            return Err("HistoryUnavailable: exact writer anchor differs from replay".into());
        }
    };
    if let Some(target_checkpoint) = exact_checkpoint {
        if replayed.anchor.checkpoint() != target_checkpoint {
            return Err(
                "HistoryUnavailable: prewarm replay differs from exact writer anchor".into(),
            );
        }
    } else if !context_matches_hint(&replayed.anchor)? {
        return Ok(None);
    }
    if std::time::Instant::now() >= deadline {
        return Ok(None);
    }

    // The staging map is bounded throughout replay. Install only after every
    // local proof and material item validated and the final checkpoint matched.
    for write in staged.into_writes() {
        reader
            .cache_content_node(&write.digest, &write.bytes)
            .map_err(|error| {
                format!("HistoryUnavailable: cache verified business node: {error}")
            })?;
    }
    if std::time::Instant::now() >= deadline {
        return Ok(None);
    }
    Ok(Some(replayed.anchor))
}

fn replayed_context_matches_hint(
    resolver: &NativeHistoryResolver,
    replayed: &VerifiedFormalReadAnchorV1,
    hint: &FormalReadAnchorRefV1,
) -> Result<bool, String> {
    let actual_authority_id = replayed
        .authority()
        .authority_id()
        .map_err(|error| error.to_string())?;
    Ok(replayed.head().record().position == hint.position
        && replayed.head().head().record_id == hint.record_id
        && actual_authority_id == hint.active_authority_id
        && replayed.head().record().world == resolver.external_origin.head().record().world
        && replayed.authority().definition() == resolver.active.definition()
        && replayed.authority().policy() == resolver.active.policy()
        && replayed.authority().activation_ref() == resolver.active.activation_ref()
        && replayed.initial_certificate() == resolver.external_origin.initial_certificate())
}

#[cfg(test)]
#[path = "formal_live_business_history_tests.rs"]
mod tests;

/// Sequential bounded reader for private proof pages already configured as
/// recovery-lineage input. Pages remain untrusted: replay verifies every
/// returned commit proof from the fixed external origin before using it.
pub(super) struct OriginHistoryProofPages {
    directory: PathBuf,
    next_page: u64,
    buffered: VecDeque<FormalQualifiedCommitProofV1>,
    exhausted: bool,
}

impl OriginHistoryProofPages {
    pub(super) fn open(directory: &Path) -> Result<Self, String> {
        crate::controlled_live_config::require_private_owned(directory, true)?;
        Ok(Self {
            directory: directory.to_owned(),
            next_page: 0,
            buffered: VecDeque::new(),
            exhausted: false,
        })
    }

    pub(super) fn proof_at(
        &mut self,
        position: u64,
    ) -> Result<Option<FormalQualifiedCommitProofV1>, super::ReplayFailure> {
        loop {
            while let Some(proof) = self.buffered.front() {
                let proof_position = proof.decision.record.position;
                if proof_position < position {
                    self.buffered.pop_front();
                    continue;
                }
                if proof_position == position {
                    return Ok(self.buffered.pop_front());
                }
                return Err(super::ReplayFailure::Invalid(format!(
                    "HistoryUnavailable: origin proof page skips requested position {position}"
                )));
            }
            if self.exhausted {
                return Ok(None);
            }

            let page_path = self.directory.join(format!("{:020}.cbor", self.next_page));
            match std::fs::symlink_metadata(&page_path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.exhausted = true;
                    return Ok(None);
                }
                Err(error) => {
                    return Err(super::ReplayFailure::Invalid(format!(
                        "HistoryUnavailable: origin proof page metadata: {error}"
                    )));
                }
                Ok(_) => crate::controlled_live_config::require_private_owned(&page_path, false)
                    .map_err(super::ReplayFailure::Invalid)?,
            }
            let bytes = crate::controlled_history_cli::read_bounded(
                &page_path,
                u64::try_from(MAX_FORMAL_PROOF_BYTES).map_err(|_| {
                    super::ReplayFailure::Invalid(
                        "HistoryUnavailable: origin proof page byte bound overflow".into(),
                    )
                })?,
            )
            .map_err(super::ReplayFailure::Invalid)?;
            let page = FormalReadHistoryPageV1::decode_bytes(&bytes).map_err(|error| {
                super::ReplayFailure::Invalid(format!(
                    "HistoryUnavailable: malformed origin proof page: {error}"
                ))
            })?;
            self.next_page = self.next_page.checked_add(1).ok_or_else(|| {
                super::ReplayFailure::Invalid(
                    "HistoryUnavailable: origin proof page index overflow".into(),
                )
            })?;
            for step in page.history.steps {
                if let FormalReadStepV1::Commit(proof) = step {
                    self.buffered.push_back(*proof);
                }
            }
        }
    }
}

#[derive(Default)]
struct StartupBusinessNodeStage {
    entries: HashMap<String, Vec<u8>>,
    order: VecDeque<String>,
    bytes: usize,
}

impl StartupBusinessNodeStage {
    fn insert(&mut self, write: BusinessIndexNodeWriteV1) -> Result<(), String> {
        if write.digest.len() != 64
            || !write
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || hex::encode(blake3::hash(&write.bytes).as_bytes()) != write.digest
        {
            return Err("HistoryUnavailable: local business CAS write hash mismatch".into());
        }
        BusinessIndexNodeV1::decode(&write.bytes)
            .map_err(|error| format!("HistoryUnavailable: local business CAS node: {error}"))?;
        if let Some(existing) = self.entries.get(&write.digest) {
            if existing != &write.bytes {
                return Err("HistoryUnavailable: local business CAS digest collision".into());
            }
            return Ok(());
        }

        let max_bytes = crate::formal_live_runtime::BUSINESS_NODE_CACHE_BYTES;
        let max_entries = crate::formal_live_runtime::BUSINESS_NODE_CACHE_ENTRIES;
        if write.bytes.len() > max_bytes || max_entries == 0 {
            return Ok(());
        }
        while self.bytes.saturating_add(write.bytes.len()) > max_bytes
            || self.entries.len() >= max_entries
        {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(evicted.len());
            }
        }
        self.bytes = self.bytes.saturating_add(write.bytes.len());
        self.order.push_back(write.digest.clone());
        self.entries.insert(write.digest, write.bytes);
        Ok(())
    }

    fn into_writes(self) -> impl Iterator<Item = BusinessIndexNodeWriteV1> {
        let mut entries = self.entries;
        self.order.into_iter().filter_map(move |digest| {
            entries
                .remove(&digest)
                .map(|bytes| BusinessIndexNodeWriteV1 { digest, bytes })
        })
    }
}

fn stage_verified_publication(
    staged: &mut StartupBusinessNodeStage,
    predecessor: &VerifiedFormalReadAnchorV1,
    anchor: &VerifiedFormalReadAnchorV1,
    proof: &FormalQualifiedCommitProofV1,
    candidate: &FormalCandidateMaterialV1,
) -> Result<(), String> {
    let record = anchor.head().record();
    let predecessor_record = predecessor.head().record();
    if predecessor.head().record().world != record.world
        || record.position != predecessor_record.position.saturating_add(1)
        || record.parent_record_id != predecessor.head().head().record_id
        || anchor.head().record() != record
        || anchor.head().head().record_id
            != record.record_id().map_err(|error| {
                format!("HistoryUnavailable: local canonical record ID: {error}")
            })?
        || proof.decision.record != *record
        || candidate.candidate.record != *record
        || candidate.branch != proof.decision.branch
        || candidate.candidate.writer_signature_hex != proof.decision.writer_signature_hex
        || candidate.frozen != proof.decision.frozen
    {
        return Err(
            "HistoryUnavailable: local publication differs from replayed qualified step".into(),
        );
    }

    let receipts_match = [
        (&proof.decision.primary.body, EndpointRole::Primary),
        (&proof.decision.replica.body, EndpointRole::Replica),
    ]
    .into_iter()
    .all(|(receipt, role)| {
        receipt.role == role
            && receipt.position == record.position
            && receipt.record_id == anchor.head().head().record_id
            && receipt.finalizing_authority_id == anchor.head().finalizing_authority_id()
            && receipt.branch == proof.decision.branch
            && receipt.recovery_certificate_digest
                == proof.decision.primary.body.recovery_certificate_digest
    });
    if !receipts_match {
        return Err(
            "HistoryUnavailable: local publication receipts differ from qualified step".into(),
        );
    }
    verify_record_material(record, &candidate.material)
        .map_err(|error| format!("HistoryUnavailable: local published material: {error}"))?;
    match (&candidate.frozen, &candidate.frozen_material) {
        (Some(certificate), Some(material)) => {
            verify_frozen_material(&certificate.reservation.body, material).map_err(|error| {
                format!("HistoryUnavailable: local published Frozen closure: {error}")
            })?;
        }
        (None, None) if record.kind == FormalRecordKindV1::InitialActivation => {}
        _ => {
            return Err("HistoryUnavailable: local published Frozen closure is incomplete".into());
        }
    }

    let result_root = candidate
        .material
        .roots
        .get(&ArtifactRole::Result)
        .ok_or("HistoryUnavailable: local published Result root missing")?;
    let result_object = candidate
        .material
        .objects
        .iter()
        .find(|object| &object.content_hash == result_root)
        .ok_or("HistoryUnavailable: local published Result object missing")?;
    if hex::encode(blake3::hash(&result_object.bytes).as_bytes()) != *result_root {
        return Err("HistoryUnavailable: local published Result artifact hash mismatch".into());
    }
    let envelope = FormalBusinessResultEnvelopeV1::decode(&result_object.bytes)
        .map_err(|error| format!("HistoryUnavailable: local business envelope: {error}"))?;
    let business_tree = FormalBusinessResultTreeV1::decode_cbor(&envelope.tree_descriptor_cbor)?;
    let metadata = business_tree.metadata();
    let delta = &envelope.delta;
    if delta.base_root != predecessor_record.result_root
        || delta
            .base
            .canonical_result_root()
            .map_err(|error| format!("HistoryUnavailable: local business base root: {error}"))?
            != delta.base_root
        || delta.new_root != record.result_root
        || delta
            .new
            .canonical_result_root()
            .map_err(|error| format!("HistoryUnavailable: local business new root: {error}"))?
            != record.result_root
        || business_tree.index_root() != &delta.new
        || business_tree.root()? != record.result_root
        || metadata.formal_position != record.position
        || metadata.world_id != record.world.world_id
        || metadata.chain_id != record.world.chain_id
        || metadata.genesis_digest != record.world.genesis_digest
        || metadata.after_state_root != record.after_state_root
        || metadata.governing_manifest_digest != record.governing_manifest_digest
    {
        return Err("HistoryUnavailable: local business tree differs from qualified record".into());
    }

    let mut writes = delta.node_writes.clone();
    if let Some(retained) = envelope.retained_history_delta {
        if retained
            .base
            .canonical_result_root()
            .map_err(|error| format!("HistoryUnavailable: retained business base root: {error}"))?
            != retained.base_root
            || retained.new.canonical_result_root().map_err(|error| {
                format!("HistoryUnavailable: retained business new root: {error}")
            })? != retained.new_root
        {
            return Err(
                "HistoryUnavailable: retained business delta roots are inconsistent".into(),
            );
        }
        writes.extend(retained.node_writes);
    }
    for write in writes {
        staged.insert(write)?;
    }
    Ok(())
}

/// Failure-only diagnostic path for crash acceptance. It independently
/// restores the supplied commit through native primary/replica verification;
/// production lookup uses `result` with the exact publication already selected
/// by the signed-view path.
#[cfg(test)]
pub(super) fn result_from_commit(
    resolver: &NativeHistoryResolver,
    cut: &CommitRef,
    correlation: &RequestCorrelation,
    scope: &str,
    reader: &str,
) -> Result<Option<(FormalBusinessResultProofV1, Option<String>)>, String> {
    let restored = resolver.restore(cut.position, Some(&cut.execution_block_hash))?;
    let anchor = resolver.anchor_for(&restored)?;
    let world = resolver.world_for(&anchor, restored.material())?;
    result(
        resolver,
        FormalReadPublication::new(anchor, world)?,
        correlation,
        scope,
        reader,
    )
}

fn historical_recovery_permission_index<'a>(
    proof: &FormalQualifiedCommitProofV1,
    permission_digests: impl Iterator<Item = &'a str>,
) -> Result<Option<usize>, String> {
    let primary = proof
        .decision
        .primary
        .body
        .recovery_certificate_digest
        .as_deref();
    let replica = proof
        .decision
        .replica
        .body
        .recovery_certificate_digest
        .as_deref();
    if proof.decision.branch == FormalCompletionBranchV1::Normal {
        if primary.is_some() || replica.is_some() {
            return Err("HistoryUnavailable: Normal historical proof has recovery receipts".into());
        }
        return Ok(None);
    }
    let digest = primary
        .filter(|digest| Some(*digest) == replica)
        .ok_or("HistoryUnavailable: historical recovery receipt digests differ or are absent")?;
    let mut matches = permission_digests
        .enumerate()
        .filter(|(_, candidate)| *candidate == digest);
    let index = matches
        .next()
        .map(|(index, _)| index)
        .ok_or("HistoryUnavailable: historical recovery permission is missing")?;
    if matches.next().is_some() {
        return Err("HistoryUnavailable: historical recovery permission is ambiguous".into());
    }
    Ok(Some(index))
}

pub(super) fn result(
    resolver: &NativeHistoryResolver,
    target: FormalReadPublication,
    correlation: &RequestCorrelation,
    scope: &str,
    reader: &str,
) -> Result<Option<(FormalBusinessResultProofV1, Option<String>)>, String> {
    #[cfg(test)]
    let timing = LookupTiming::new();
    let (anchor, world) = target.into_verified_cut();
    #[cfg(test)]
    let stage = timing.enter("current_material");
    let current_material = material_for_target(resolver, &anchor)?;
    #[cfg(test)]
    timing.finish("current_material", stage);

    #[cfg(test)]
    let stage = timing.enter("current_result_proof");
    let publication =
        FormalBusinessResultPublication::new(anchor.clone(), world, tree(&current_material)?)?;
    let Some(current_proof) =
        publication.proof_for_if_present(correlation, scope, reader, resolver)?
    else {
        #[cfg(test)]
        timing.finish("current_result_proof", stage);
        return Ok(None);
    };
    #[cfg(test)]
    timing.finish("current_result_proof", stage);
    let position = current_proof.outcome.first_formal_position;
    if position == anchor.head().record().position {
        return Ok(Some((current_proof, None)));
    }

    #[cfg(test)]
    let stage = timing.enter("current_root_inclusion");
    let inclusion = publication.tree().historical_cut_inclusion(
        &anchor.head().record().world,
        position,
        resolver,
    )?;
    let included = VerifiedHistoricalCutInclusionV2::verify(
        &anchor,
        publication.tree().index_root(),
        &inclusion,
        position,
    )
    .map_err(|error| error.to_string())?;
    #[cfg(test)]
    timing.finish("current_root_inclusion", stage);

    #[cfg(test)]
    let stage = timing.enter("historical_cut_qualification");
    let historical = if included
        .cut()
        .parent_qualified
        .decision
        .primary
        .body
        .recovery_certificate_digest
        .is_some()
    {
        included
            .qualify_with_verified_recovery_lineage(&anchor, &resolver.permissions)
            .map_err(|error| error.to_string())?
    } else {
        let historical_authority = resolver
            .authority_for_record(&included.cut().parent_record)
            .ok_or_else(|| {
                "HistoryUnavailable: historical Normal authority is outside verified lineage"
                    .to_owned()
            })?;
        included
            .qualify_normal_with_verified_authority(&anchor, historical_authority)
            .map_err(|error| error.to_string())?
    };
    #[cfg(test)]
    timing.finish("historical_cut_qualification", stage);

    #[cfg(test)]
    let stage = timing.enter("historical_anchor_and_local_seed");
    let old_anchor = if historical
        .proof()
        .decision
        .primary
        .body
        .recovery_certificate_digest
        .is_some()
    {
        VerifiedFormalReadAnchorV1::from_committed_historical_recovery_cut(&anchor, &historical)
    } else {
        VerifiedFormalReadAnchorV1::from_committed_historical_cut_with_verified_authority(
            &anchor,
            &historical,
        )
    }
    .map_err(|error| error.to_string())?;
    seed_published_business_nodes_for_cut(resolver, &historical, &old_anchor)?;
    #[cfg(test)]
    timing.finish("historical_anchor_and_local_seed", stage);

    let recovery_index = historical_recovery_permission_index(
        historical.proof(),
        resolver
            .permissions
            .iter()
            .map(|permission| permission.recovery().certificate_digest()),
    )?;
    let recovery = recovery_index.map(|index| &resolver.permissions[index]);
    let original = recovery.map_or(historical.authority(), |permission| {
        permission.original_authority()
    });
    let restore = |address, role| -> Result<_, String> {
        // Retain the current publication as the transport floor. NativeAt only
        // selects the already qualified old cut; it cannot mint a new anchor.
        let restored = RemoteFormalEndpoint::new(
            address,
            resolver.timeout,
            anchor.authority().clone(),
            role,
            resolver.authentication.clone(),
            anchor.head().head(),
        )
        .map_err(|error| error.to_string())?
        .restore_native_at_cached(
            &historical.commit().head(),
            historical.authority(),
            original,
            recovery.map(|permission| permission.recovery()),
            resolver,
        )
        .map_err(|error| error.to_string())?;
        if restored.minimum() != &anchor.head().head()
            || restored.commit().record() != historical.commit().record()
            || restored.commit().head() != historical.commit().head()
            || restored.commit().finalizing_authority_id()
                != historical.commit().finalizing_authority_id()
            || restored.commit().record() != old_anchor.head().record()
            || restored.commit().head() != old_anchor.head().head()
            || restored.commit().finalizing_authority_id()
                != old_anchor.head().finalizing_authority_id()
            || restored.proof() != historical.proof()
            || restored
                .authority()
                .context_digest()
                .map_err(|error| error.to_string())?
                != old_anchor
                    .authority()
                    .context_digest()
                    .map_err(|error| error.to_string())?
        {
            return Err("HistoryUnavailable: native original material differs from the qualified historical cut".into());
        }
        Ok(restored)
    };
    #[cfg(test)]
    let stage = timing.enter("primary_protected_restore");
    let primary = restore(resolver.addresses[0], EndpointRole::Primary)?;
    #[cfg(test)]
    timing.finish("primary_protected_restore", stage);

    #[cfg(test)]
    let stage = timing.enter("replica_protected_restore");
    let replica = restore(resolver.addresses[1], EndpointRole::Replica)?;
    #[cfg(test)]
    timing.finish("replica_protected_restore", stage);
    if primary.candidate() != replica.candidate() || primary.proof() != replica.proof() {
        return Err("OutcomeUnknown: original result replicas disagree".into());
    }

    #[cfg(test)]
    let stage = timing.enter("original_result_and_proofs");
    let old_world = resolver.world_for(&old_anchor, primary.material())?;
    let original = FormalBusinessResultPublication::new(
        old_anchor.clone(),
        old_world.clone(),
        tree(primary.material())?,
    )?;
    let (current_proof, original_proof) = publication.proofs_for_committed_history(
        &original,
        &historical,
        correlation,
        scope,
        reader,
        resolver,
    )?;
    #[cfg(test)]
    timing.finish("original_result_and_proofs", stage);

    #[cfg(test)]
    let stage = timing.enter("historical_view_and_encode");
    let request = ReadWorldViewRequest {
        observation_nonce: Some("07".repeat(32)),
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        world: correlation.key.world.clone(),
        scope_id: "public".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
        formal_anchor: Some(FormalReadAnchorRefV1 {
            position,
            record_id: old_anchor.head().head().record_id,
            active_authority_id: old_anchor
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?,
        }),
    };
    let view =
        FormalReadPublication::new(old_anchor, old_world)?.view_at_own_anchor(&request, reader)?;
    let encoded = FormalHistoricalBusinessPublicationV1 {
        schema_version: 2,
        current_index_root: publication.tree().index_root().clone(),
        cut_inclusion: inclusion,
        current_result: current_proof,
        original_request: request,
        original_view: view,
    }
    .encode_hex()?;
    #[cfg(test)]
    timing.finish("historical_view_and_encode", stage);
    Ok(Some((original_proof, Some(encoded))))
}

/// Seed only immutable bytes from a private publication whose complete proof is
/// already selected by the current-root historical inclusion. This skips CAS
/// object RPCs only; both protected endpoint Queries below still qualify the
/// old candidate and material independently.
fn seed_published_business_nodes_for_cut(
    resolver: &NativeHistoryResolver,
    cut: &oasis7_distfs::controlled_authority::formal::VerifiedCommittedHistoricalCutV1,
    anchor: &VerifiedFormalReadAnchorV1,
) -> Result<bool, String> {
    let record = cut.commit().record();
    let verified_head = cut.commit().head();
    if anchor.head().record() != record
        || anchor.head().head() != verified_head
        || anchor.head().finalizing_authority_id() != cut.commit().finalizing_authority_id()
        || cut.proof().decision.record != *record
    {
        return Err("HistoryUnavailable: historical cache cut differs from verified anchor".into());
    }

    let Some((candidate, proof)) =
        super::super::execution_bridge::formal_writer::read_published_candidate_at(
            &resolver.writer_directory,
            record.position,
        )?
    else {
        return Ok(false);
    };
    let record_id = verified_head.record_id.as_str();
    let receipts_match = [
        (&proof.decision.primary.body, EndpointRole::Primary),
        (&proof.decision.replica.body, EndpointRole::Replica),
    ]
    .into_iter()
    .all(|(receipt, role)| {
        receipt.role == role
            && receipt.position == record.position
            && receipt.record_id == record_id
            && receipt.finalizing_authority_id == cut.commit().finalizing_authority_id()
            && receipt.branch == cut.proof().decision.branch
            && receipt.recovery_certificate_digest
                == cut
                    .proof()
                    .decision
                    .primary
                    .body
                    .recovery_certificate_digest
    });
    if proof != *cut.proof()
        || candidate.candidate.record != *record
        || proof.decision.record != *record
        || candidate.branch != proof.decision.branch
        || candidate.candidate.writer_signature_hex != proof.decision.writer_signature_hex
        || candidate.frozen != proof.decision.frozen
        || !receipts_match
    {
        return Err(
            "HistoryUnavailable: local published proof differs from exact historical cut".into(),
        );
    }
    verify_record_material(record, &candidate.material)
        .map_err(|error| format!("HistoryUnavailable: local historical material: {error}"))?;
    match (&candidate.frozen, &candidate.frozen_material) {
        (Some(certificate), Some(material)) => {
            verify_frozen_material(&certificate.reservation.body, material)
                .map_err(|error| format!("HistoryUnavailable: local Frozen closure: {error}"))?
        }
        (None, None) if record.kind == FormalRecordKindV1::InitialActivation => {}
        _ => {
            return Err("HistoryUnavailable: local historical Frozen closure is incomplete".into());
        }
    }

    let result_root = candidate
        .material
        .roots
        .get(&ArtifactRole::Result)
        .ok_or("HistoryUnavailable: local historical Result root is missing")?;
    let result_object = candidate
        .material
        .objects
        .iter()
        .find(|object| &object.content_hash == result_root)
        .ok_or("HistoryUnavailable: local historical Result object is missing")?;
    if hex::encode(blake3::hash(&result_object.bytes).as_bytes()) != *result_root {
        return Err("HistoryUnavailable: local historical Result hash mismatch".into());
    }
    let envelope = FormalBusinessResultEnvelopeV1::decode(&result_object.bytes)
        .map_err(|error| format!("HistoryUnavailable: local business envelope: {error}"))?;
    let business_tree = FormalBusinessResultTreeV1::decode_cbor(&envelope.tree_descriptor_cbor)?;
    if envelope.delta.new_root != record.result_root
        || envelope
            .delta
            .new
            .canonical_result_root()
            .map_err(|error| {
                format!("HistoryUnavailable: local business root descriptor: {error}")
            })?
            != record.result_root
        || business_tree.root()? != record.result_root
    {
        return Err("HistoryUnavailable: local business material differs from signed root".into());
    }

    let mut writes: Vec<BusinessIndexNodeWriteV1> = envelope.delta.node_writes;
    if let Some(retained) = envelope.retained_history_delta {
        writes.extend(retained.node_writes);
    }
    let mut unique = std::collections::BTreeMap::<String, Vec<u8>>::new();
    let mut total_bytes = 0usize;
    for write in writes {
        if write.digest.len() != 64
            || !write
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || hex::encode(blake3::hash(&write.bytes).as_bytes()) != write.digest
        {
            return Err("HistoryUnavailable: local business CAS write hash mismatch".into());
        }
        if let Some(existing) = unique.get(&write.digest) {
            if existing != &write.bytes {
                return Err("HistoryUnavailable: local business CAS digest collision".into());
            }
            continue;
        }
        total_bytes = total_bytes
            .checked_add(write.bytes.len())
            .ok_or("HistoryUnavailable: local business CAS byte count overflow")?;
        if total_bytes > MAX_VERIFIED_BUSINESS_SEED_BYTES {
            return Err("HistoryUnavailable: local business CAS seed exceeds bound".into());
        }
        unique.insert(write.digest, write.bytes);
    }
    for (digest, bytes) in unique {
        resolver
            .business_reader
            .cache_content_node(&digest, &bytes)
            .map_err(|error| {
                format!("HistoryUnavailable: cache verified business node: {error}")
            })?;
    }
    Ok(true)
}

/// Reuse private local material only at the exact typed publication selected
/// by the signed-read verifier. Local bytes never establish authority: their
/// complete canonical record must equal the opaque anchor head and the closed
/// material commitments and result tree are checked again. A missing local
/// entry retains the dual protected native-restore path.
fn material_for_target(
    resolver: &NativeHistoryResolver,
    anchor: &VerifiedFormalReadAnchorV1,
) -> Result<ClosedRecord, String> {
    let position = anchor.head().record().position;
    if let Some((candidate, proof)) =
        super::super::execution_bridge::formal_writer::read_published_candidate_at(
            &resolver.writer_directory,
            position,
        )?
    {
        let record = anchor.head().record();
        let head = anchor.head().head();
        let record_id = head.record_id.as_str();
        let receipts_match = [
            (&proof.decision.primary.body, EndpointRole::Primary),
            (&proof.decision.replica.body, EndpointRole::Replica),
        ]
        .into_iter()
        .all(|(receipt, role)| {
            receipt.role == role
                && receipt.position == position
                && receipt.record_id.as_str() == record_id
                && receipt.finalizing_authority_id.as_str()
                    == anchor.head().finalizing_authority_id()
        });
        if candidate.candidate.record != *record
            || proof.decision.record != *record
            || candidate.branch != proof.decision.branch
            || candidate.candidate.writer_signature_hex != proof.decision.writer_signature_hex
            || candidate.frozen != proof.decision.frozen
            || !receipts_match
        {
            return Err(
                "HistoryUnavailable: local published material differs from exact verified cut"
                    .into(),
            );
        }
        verify_record_material(record, &candidate.material)
            .map_err(|error| format!("HistoryUnavailable: local published material: {error}"))?;
        return Ok(candidate.material);
    }

    let head = anchor.head().head();
    let record_id = head.record_id.as_str();
    let restored = resolver.restore(position, Some(record_id))?;
    if restored.commit().record() != anchor.head().record()
        || restored.commit().head() != anchor.head().head()
        || restored.commit().finalizing_authority_id() != anchor.head().finalizing_authority_id()
    {
        return Err(
            "HistoryUnavailable: native result material differs from exact verified cut".into(),
        );
    }
    Ok(restored.material().clone())
}

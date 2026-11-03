//! Immutable private closure exported from actual durable wire-3 execution.
//! It is replay input, never a replacement for the protected minimum.
use super::*;
#[path = "driver_bft_v3_protected_retention_archive.rs"]
pub(super) mod protected_retention;
#[path = "driver_bft_v3_retention_cut.rs"]
mod retention_cut;
use oasis7_distfs::protected_bft_v3::{
    BftSignerStateV3, BftSuccessorWindowArchiveEntryV1, BftSuccessorWindowArchiveKindV1,
    BftSuccessorWindowArchiveManifestV1, BftSuccessorWindowArchiveRefV1,
    BftSuccessorWindowArchiveUsageV1, BftSuccessorWindowCutV1, BftSuccessorWindowSourceV1,
    LocalBftSuccessorArchiveStoreV1,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

pub(super) const MAX_METADATA: usize = 1024 * 1024;
pub(super) const MAX_CLOSURE: usize = 512 * 1024 * 1024;
pub(super) const MAX_REFS: usize = 4096;
const DOMAIN: &str = "oasis7.formal.bft.successor-private-closure.v1";
pub(super) const WINDOW_DOMAIN: &str = "oasis7.formal.bft.successor-private-closure.v2";
pub(super) const PUBLICATION_DOMAIN: &str = "oasis7.formal.bft.successor-private-publication.v1";
pub(super) const PUBLICATION_WINDOW_DOMAIN: &str =
    "oasis7.formal.bft.successor-private-publication-window.v1";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PendingRoleV1 {
    RetainedLocal,
    Proposal,
    Equivocation,
    BlockPrevote,
    BlockPrecommit,
    PendingProposal,
    PendingVote,
    PendingCommit,
    ProtectedOriginal,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingCandidateV1 {
    pub height: u64,
    pub header_id: String,
    pub roles: BTreeSet<PendingRoleV1>,
    pub value: oasis7_consensus::tendermint_v3::BftHeaderValueV3,
    pub actions: BftSuccessorWindowArchiveRefV1,
    #[serde(deserialize_with = "bounded_refs")]
    pub dependencies: Vec<BftSuccessorWindowArchiveRefV1>,
}
pub(super) fn required_pending(
    journal: Option<&oasis7_consensus::tendermint_v3::BftSigningJournalV3>,
) -> Result<BTreeMap<String, BTreeSet<PendingRoleV1>>, String> {
    use oasis7_consensus::tendermint_v3::BftSigningJournalEventV3 as Event;
    use oasis7_proto::distributed_bft::v3::{BftVotePhaseV3, BftVoteValueV3};
    let mut ids = BTreeMap::<String, BTreeSet<PendingRoleV1>>::new();
    let Some(journal) = journal else {
        return Ok(ids);
    };
    let mut add = |id: &str, role: PendingRoleV1| {
        ids.entry(id.to_owned()).or_default().insert(role);
    };
    for event in journal.events() {
        match event {
            Event::Proposal { proposal, .. } => {
                add(&proposal.block.block_id()?, PendingRoleV1::Proposal)
            }
            Event::ProposalEquivocation { first, second, .. } => {
                add(&first.block_id, PendingRoleV1::Equivocation);
                add(&second.block_id, PendingRoleV1::Equivocation);
            }
            Event::Vote { vote, .. } => {
                if let BftVoteValueV3::Block { block_id } = &vote.value {
                    add(
                        block_id,
                        if vote.phase == BftVotePhaseV3::Prevote {
                            PendingRoleV1::BlockPrevote
                        } else {
                            PendingRoleV1::BlockPrecommit
                        },
                    );
                }
            }
            Event::Timeout { .. } => {}
        }
    }
    if let Some(intent) = journal.pending_proposal_intent() {
        add(&intent.block.block_id()?, PendingRoleV1::PendingProposal);
    }
    if let Some(intent) = journal.pending_vote_intent()
        && let BftVoteValueV3::Block { block_id } = &intent.value
    {
        add(block_id, PendingRoleV1::PendingVote);
    }
    if let Some(certificate) = journal.pending_commit() {
        add(
            &certificate.header.block_id()?,
            PendingRoleV1::PendingCommit,
        );
    }
    if ids.len() > 16 {
        return Err("successor T+1 private candidate capacity".into());
    }
    Ok(ids)
}

/// An actual applied cut may carry its original, already-certified next value.
/// The next QC is never the cut: independently check it against the actual
/// replayed schedule and retain its private package as an explicit role.
pub(super) fn validate_original_pending_commit(
    state: &BftSignerStateV3,
    schedule: &BftHeaderHeightScheduleV3,
    cut_height: u64,
    cut_header_id: &str,
    cut_install_digest: &str,
) -> Result<(), String> {
    let Some(journal) = state.journal.as_ref() else {
        return Ok(());
    };
    let Some(certificate) = journal.pending_commit() else {
        return Ok(());
    };
    if state.history.is_empty()
        || state.pending_driver_apply.is_some()
        || state.history.last().is_none_or(|cut| {
            cut.execution_commitment.height != cut_height
                || cut.header.block_id().as_deref() != Ok(cut_header_id)
        })
        || state.applied_installs.last().is_none_or(|applied| {
            applied.height != cut_height
                || applied.header_id != cut_header_id
                || applied.driver_install_digest != cut_install_digest
        })
        || cut_height.checked_add(1) != Some(schedule.height())
        || journal.height() != schedule.height()
        || journal.parent_commit_ref() != cut_header_id
        || journal.schedule_digest != schedule.digest()?
    {
        return Err("successor original pending commit lacks the actual applied cut".into());
    }
    oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3(schedule, certificate)?;
    Ok(())
}

pub(super) fn validate_pending_commit_value(
    state: &BftSignerStateV3,
    roles: &BTreeSet<PendingRoleV1>,
    value: &oasis7_consensus::tendermint_v3::BftHeaderValueV3,
) -> Result<(), String> {
    if roles.contains(&PendingRoleV1::PendingCommit) {
        let certificate = state
            .journal
            .as_ref()
            .and_then(|journal| journal.pending_commit())
            .ok_or("successor PendingCommit role lacks original certificate")?;
        if value.header != certificate.header
            || value.execution_commitment != certificate.execution_commitment
        {
            return Err(
                "successor pending commit private value differs from original certificate".into(),
            );
        }
    }
    Ok(())
}
fn bounded_pending<'de, D: serde::Deserializer<'de>>(
    de: D,
) -> Result<Vec<PendingCandidateV1>, D::Error> {
    struct Pending;
    impl<'de> serde::de::Visitor<'de> for Pending {
        type Value = Vec<PendingCandidateV1>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("at most 16 original T+1 candidates")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > 16) {
                return Err(serde::de::Error::custom("pending candidate capacity"));
            }
            let mut values = Vec::new();
            while let Some(value) = seq.next_element()? {
                if values.len() == 16 {
                    return Err(serde::de::Error::custom("pending candidate capacity"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    de.deserialize_seq(Pending)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrivateWindowBaseV1 {
    pub checkpoint_digest: String,
    pub window_origin: BftSuccessorWindowArchiveRefV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArchivedPublicationV1 {
    pub data: BftSuccessorWindowArchiveRefV1,
    pub original_data: Option<BftSuccessorWindowArchiveRefV1>,
    pub reservation: BftSuccessorWindowArchiveRefV1,
}
impl ArchivedPublicationV1 {
    pub(super) fn references(&self) -> impl Iterator<Item = &BftSuccessorWindowArchiveRefV1> {
        std::iter::once(&self.data)
            .chain(self.original_data.iter())
            .chain(std::iter::once(&self.reservation))
    }
}
fn bounded_publications<'de, D: serde::Deserializer<'de>>(
    de: D,
) -> Result<Vec<ArchivedPublicationV1>, D::Error> {
    struct Items;
    impl<'de> serde::de::Visitor<'de> for Items {
        type Value = Vec<ArchivedPublicationV1>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("at most 256 original publication bindings")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > 256) {
                return Err(serde::de::Error::custom("publication archive count"));
            }
            let mut items = Vec::new();
            while let Some(item) = seq.next_element()? {
                if items.len() == 256 {
                    return Err(serde::de::Error::custom("publication archive count"));
                }
                items.push(item);
            }
            Ok(items)
        }
    }
    de.deserialize_seq(Items)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrivateClosureV1 {
    pub domain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_layout: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_base: Option<PrivateWindowBaseV1>,
    pub source: BftSuccessorWindowSourceV1,
    pub private_instance_id: String,
    pub material: BftSuccessorWindowArchiveRefV1,
    pub origin: BftSuccessorWindowArchiveRefV1,
    pub public_state: BftSuccessorWindowArchiveRefV1,
    pub current_journal: Option<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_pending")]
    pub pending_candidates: Vec<PendingCandidateV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub snapshots: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub journals: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub manifests: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub records: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub nodes: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "bounded_publications"
    )]
    pub publications: Vec<ArchivedPublicationV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_retention: Option<BftSuccessorWindowArchiveRefV1>,
    pub cut: BftSuccessorWindowCutV1,
}
fn bounded_refs<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BftSuccessorWindowArchiveRefV1>, D::Error> {
    struct Refs;
    impl<'de> serde::de::Visitor<'de> for Refs {
        type Value = Vec<BftSuccessorWindowArchiveRefV1>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded successor private closure references")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_REFS) {
                return Err(serde::de::Error::custom(
                    "successor private reference bound",
                ));
            }
            let mut refs = Vec::new();
            while let Some(reference) = seq.next_element()? {
                if refs.len() == MAX_REFS {
                    return Err(serde::de::Error::custom(
                        "successor private reference bound",
                    ));
                }
                refs.push(reference);
            }
            Ok(refs)
        }
    }
    deserializer.deserialize_seq(Refs)
}
impl PrivateClosureV1 {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.source.validate().map_err(err)?;
        self.cut.validate().map_err(err)?;
        let domain = match (self.publication_layout, self.active_base.is_some()) {
            (None, false) => DOMAIN,
            (None, true) => WINDOW_DOMAIN,
            (Some(1), false) => PUBLICATION_DOMAIN,
            (Some(1), true) => PUBLICATION_WINDOW_DOMAIN,
            (Some(2), false) => protected_retention::PROTECTED_DOMAIN,
            _ => return Err("unsupported private publication layout".into()),
        };
        if self.domain != domain
            || (self.publication_layout.is_none() && !self.publications.is_empty())
            || (self.protected_retention.is_some() != (self.publication_layout == Some(2)))
            || self.private_instance_id.is_empty()
            || self.private_instance_id.len() > 256
            || self.records.is_empty()
            || self.records.len() > 64
            || self.nodes.len() > MAX_REFS
            || self.snapshots.len() != self.records.len()
            || self.journals.len() != self.records.len()
            || self.manifests.len() != self.records.len()
        {
            return Err("successor private closure bounds/domain".into());
        }
        if let Some(base) = &self.active_base {
            base.window_origin.validate().map_err(err)?;
            if base.checkpoint_digest.len() != 64
                || !base
                    .checkpoint_digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("private archive predecessor checkpoint encoding".into());
            }
        }
        let mut total = 0u64;
        for reference in std::iter::once(&self.material)
            .chain(std::iter::once(&self.origin))
            .chain(std::iter::once(&self.public_state))
            .chain(self.current_journal.iter())
            .chain(self.protected_retention.iter())
            .chain(&self.records)
            .chain(&self.nodes)
            .chain(&self.snapshots)
            .chain(&self.journals)
            .chain(&self.manifests)
        {
            reference.validate().map_err(err)?;
            total = total
                .checked_add(reference.bytes)
                .filter(|n| *n <= MAX_CLOSURE as u64)
                .ok_or("successor private cumulative capacity")?;
        }
        if self.publications.len() > 256
            || !self
                .publications
                .windows(2)
                .all(|p| p[0].data.digest < p[1].data.digest)
        {
            return Err("publication archive order/count".into());
        }
        for publication in &self.publications {
            for reference in publication.references() {
                reference.validate().map_err(err)?;
                total = total
                    .checked_add(reference.bytes)
                    .filter(|n| *n <= MAX_CLOSURE as u64)
                    .ok_or("publication archive cumulative capacity")?;
            }
        }
        if !self.nodes.windows(2).all(|p| p[0].digest < p[1].digest) {
            return Err("successor private index references not sorted unique".into());
        }
        if self.pending_candidates.len() > 16
            || !self
                .pending_candidates
                .windows(2)
                .all(|p| p[0].header_id < p[1].header_id)
        {
            return Err("successor pending candidate order/count".into());
        }
        if self
            .pending_candidates
            .iter()
            .filter(|c| c.roles.contains(&PendingRoleV1::RetainedLocal))
            .count()
            > 1
        {
            return Err("successor archive has multiple original local selections".into());
        }
        for candidate in &self.pending_candidates {
            if candidate.roles.contains(&PendingRoleV1::ProtectedOriginal)
                && self.publication_layout != Some(2)
            {
                return Err("protected original role in legacy private archive".into());
            }
            candidate.value.validate_binding()?;
            if candidate.height
                != self
                    .cut
                    .height
                    .checked_add(1)
                    .ok_or("pending height overflow")?
                || candidate.header_id != candidate.value.block_id()?
                || candidate.roles.is_empty()
                || candidate.value.execution_commitment.height != candidate.height
                || !candidate
                    .dependencies
                    .windows(2)
                    .all(|p| p[0].digest < p[1].digest)
            {
                return Err("successor pending candidate binding".into());
            }
            for reference in std::iter::once(&candidate.actions).chain(&candidate.dependencies) {
                reference.validate().map_err(err)?;
            }
        }
        Ok(())
    }
}
/// Fields are private so only an actual durable exporter can mint this receipt.
pub(crate) struct ExportedBftSuccessorPrivateClosureV1 {
    pub(super) manifest: BftSuccessorWindowArchiveManifestV1,
    pub(super) manifest_ref: BftSuccessorWindowArchiveRefV1,
    pub(super) metadata_ref: BftSuccessorWindowArchiveRefV1,
    pub(super) public_state_ref: BftSuccessorWindowArchiveRefV1,
    pub(super) pending_actions: Vec<BftSuccessorWindowArchiveRefV1>,
    pub(super) cut: BftSuccessorWindowCutV1,
}
impl ExportedBftSuccessorPrivateClosureV1 {
    pub(crate) fn manifest(&self) -> &BftSuccessorWindowArchiveManifestV1 {
        &self.manifest
    }
    pub(crate) fn execution_manifest_ref(&self) -> &BftSuccessorWindowArchiveRefV1 {
        &self.manifest_ref
    }
    pub(crate) fn metadata_ref(&self) -> &BftSuccessorWindowArchiveRefV1 {
        &self.metadata_ref
    }
    pub(crate) fn public_state_ref(&self) -> &BftSuccessorWindowArchiveRefV1 {
        &self.public_state_ref
    }
    pub(crate) fn pending_actions(&self) -> &[BftSuccessorWindowArchiveRefV1] {
        &self.pending_actions
    }
    pub(crate) fn cut(&self) -> &BftSuccessorWindowCutV1 {
        &self.cut
    }

    /// Test-only caller-selected omission; never creates a verified capability.
    #[cfg(test)]
    pub(crate) fn omit_pending_publication_for_test(
        &self,
        archive: &LocalBftSuccessorArchiveStoreV1,
        height: u64,
    ) -> Result<(Vec<u8>, Vec<BftSuccessorWindowArchiveRefV1>), String> {
        let original = archive
            .load_object(&self.metadata_ref, MAX_METADATA)
            .map_err(err)?;
        let mut metadata: PrivateClosureV1 = decode(&original, MAX_METADATA)?;
        if encode(&metadata, MAX_METADATA)? != original {
            return Err("test fixture must use original typed metadata codec".into());
        }
        let mut omitted = Vec::new();
        let mut kept = Vec::new();
        for publication in metadata.publications {
            let bytes = archive
                .load_object(
                    &publication.data,
                    super::super::super::formal_ledger::MAX_RECORD,
                )
                .map_err(err)?;
            let data: serde_cbor::Value = serde_cbor::from_slice(&bytes).map_err(err)?;
            let serde_cbor::Value::Map(fields) = data else {
                return Err("test original Data map missing".into());
            };
            if fields.get(&serde_cbor::Value::Text("height".into()))
                == Some(&serde_cbor::Value::Integer(height.into()))
            {
                if !matches!(fields.get(&serde_cbor::Value::Text("publication_reservation_ref".into())), Some(serde_cbor::Value::Text(reference)) if reference == &publication.reservation.digest)
                {
                    return Err(
                        "test target original Data must contain exact Some reservation".into(),
                    );
                }
                omitted.push(publication);
            } else {
                kept.push(publication);
            }
        }
        if omitted.len() != 1 {
            return Err("test requires one original pending Some publication".into());
        }
        metadata.publications = kept;
        let mut still_required = BTreeSet::new();
        for reference in std::iter::once(&metadata.material)
            .chain(std::iter::once(&metadata.origin))
            .chain(std::iter::once(&metadata.public_state))
            .chain(metadata.current_journal.iter())
            .chain(&metadata.records)
            .chain(&metadata.nodes)
            .chain(&metadata.snapshots)
            .chain(&metadata.journals)
            .chain(&metadata.manifests)
            .chain(metadata.publications.iter().flat_map(|p| p.references()))
        {
            still_required.insert(reference.digest.clone());
        }
        if let Some(base) = &metadata.active_base {
            still_required.insert(base.window_origin.digest.clone());
        }
        for pending in &metadata.pending_candidates {
            for reference in std::iter::once(&pending.actions).chain(&pending.dependencies) {
                still_required.insert(reference.digest.clone());
            }
        }
        let removed = omitted[0]
            .references()
            .filter(|reference| !still_required.contains(&reference.digest))
            .cloned()
            .collect();
        metadata.validate()?;
        Ok((encode(&metadata, MAX_METADATA)?, removed))
    }
}
pub(super) fn publication_layout_reserve(layout: Option<u32>) -> Result<(u64, u64), String> {
    match layout {
        None => Ok((0, 0)),
        Some(1 | 2) => node_execution::auto_prepare_reserve(),
        _ => Err("unsupported private publication reserve layout".into()),
    }
}
pub(super) fn metadata_identity() -> String {
    blake3_hex(DOMAIN.as_bytes())
}
pub(super) fn err(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}
pub(super) fn live(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("successor private closure deadline expired".into())
    } else {
        Ok(())
    }
}
pub(super) fn encode(value: &impl Serialize, max: usize) -> Result<Vec<u8>, String> {
    let bytes = serde_cbor::to_vec(value).map_err(err)?;
    if bytes.is_empty() || bytes.len() > max {
        return Err("successor private encoding bound".into());
    }
    Ok(bytes)
}
pub(super) fn encode_protocol(value: &impl Serialize, max: usize) -> Result<Vec<u8>, String> {
    let bytes = oasis7_cas::to_canonical_cbor(value).map_err(err)?;
    if bytes.is_empty() || bytes.len() > max {
        return Err("successor protocol encoding bound".into());
    }
    Ok(bytes)
}
pub(super) fn decode<T: serde::de::DeserializeOwned + Serialize>(
    bytes: &[u8],
    max: usize,
) -> Result<T, String> {
    if bytes.is_empty() || bytes.len() > max {
        return Err("successor private decode bound".into());
    }
    let value: T = serde_cbor::from_slice(bytes).map_err(err)?;
    if encode(&value, max)? != bytes {
        return Err("successor private noncanonical encoding".into());
    }
    Ok(value)
}
/// Permanent conservative reservation for the next segment's immutable init
/// metadata and copied original candidate WAL/CAS records. It is not refunded.
#[path = "driver_bft_v3_carry_initialization_plan.rs"]
pub(super) mod carry_initialization;
pub(super) fn window_init_reserve(pending: &[PendingCandidateV1]) -> Result<(u64, u64), String> {
    let mut bytes = 8 * MAX_METADATA as u64;
    let mut objects = 16u64;
    for candidate in pending {
        let data = candidate
            .actions
            .bytes
            .checked_mul(2)
            .and_then(|n| n.checked_add(candidate.dependencies.len() as u64 * 128))
            .and_then(|n| n.checked_add(16 * 1024))
            .ok_or("active init candidate accounting overflow")?;
        bytes = bytes
            .checked_add(
                data.checked_mul(4)
                    .ok_or("active init WAL accounting overflow")?,
            )
            .filter(|n| *n <= MAX_CLOSURE as u64)
            .ok_or("active init material capacity")?;
        objects = objects
            .checked_add(4)
            .ok_or("active init object accounting overflow")?;
    }
    Ok((bytes, objects))
}

pub(super) fn install_digest(
    material: &str,
    record: &str,
    certificate: &FormalBftCertifiedHeaderV3,
) -> Result<String, String> {
    let c = &certificate.execution_commitment;
    Ok(blake3_hex(
        &serde_cbor::to_vec(&(
            "oasis7.formal.bft.node_install.v3",
            material,
            record,
            certificate.header.block_id()?,
            &certificate.header.execution_commitment_hash,
            &c.execution_state_root,
            &c.state_root,
            &c.post_manifest_hash,
        ))
        .map_err(err)?,
    ))
}

pub(crate) struct SuccessorPrivateExportRequestV1<'a> {
    pub(crate) archive: &'a LocalBftSuccessorArchiveStoreV1,
    pub(crate) source: &'a BftSuccessorWindowSourceV1,
    pub(crate) public_state: &'a oasis7_distfs::protected_bft_v3::BftSignerStateV3,
    pub(crate) previous_usage: BftSuccessorWindowArchiveUsageV1,
    pub(crate) height: u64,
    pub(crate) header_id: &'a str,
    pub(crate) sequence: u64,
    pub(crate) predecessor: Option<String>,
    pub(crate) deadline: Instant,
}
impl FormalBftHeaderLedgerExecutionV3 {
    pub(super) fn export_successor_private_closure(
        &mut self,
        request: SuccessorPrivateExportRequestV1<'_>,
        private_instance_id: &str,
        pending_packages: &[(
            BTreeSet<PendingRoleV1>,
            oasis7_node::NodeBftPrivateCandidatePackageV3,
        )],
        publication_materials: &[node_execution::PublicationArchiveMaterialV1],
    ) -> Result<ExportedBftSuccessorPrivateClosureV1, String> {
        self.export_successor_private_closure_with_protected(
            request,
            private_instance_id,
            pending_packages,
            publication_materials,
            None,
        )
    }

    pub(super) fn export_successor_private_closure_with_protected(
        &mut self,
        request: SuccessorPrivateExportRequestV1<'_>,
        private_instance_id: &str,
        pending_packages: &[(
            BTreeSet<PendingRoleV1>,
            oasis7_node::NodeBftPrivateCandidatePackageV3,
        )],
        publication_materials: &[node_execution::PublicationArchiveMaterialV1],
        protected: Option<(
            &protected_retention::ValidatedProtectedRetentionArchiveV1,
            &BftSuccessorWindowArchiveRefV1,
        )>,
    ) -> Result<ExportedBftSuccessorPrivateClosureV1, String> {
        let SuccessorPrivateExportRequestV1 {
            archive,
            source,
            public_state,
            previous_usage,
            height,
            header_id,
            sequence,
            predecessor: predecessor_root,
            deadline,
        } = request;
        live(deadline)?;
        let cut = self.verified_cut(height, header_id)?;
        live(deadline)?;
        source.validate().map_err(err)?;
        if public_state.binding_digest != source.binding_digest
            || public_state.pending_driver_apply.is_some()
            || public_state.history.last() != Some(cut.proof().certificate().certified_header())
        {
            return Err("successor private original S differs from installed cut".into());
        }
        validate_original_pending_commit(
            public_state,
            &self.origin.schedule,
            height,
            header_id,
            &install_digest(
                &source.material_identity,
                cut.durable_record_digest(),
                cut.proof().certificate().certified_header(),
            )?,
        )?;
        for (roles, package) in pending_packages {
            validate_pending_commit_value(public_state, roles, &package.candidate.public_value)?;
        }
        if public_state
            .journal
            .as_ref()
            .and_then(|journal| journal.pending_commit())
            .is_some()
            && pending_packages
                .iter()
                .filter(|(roles, _)| roles.contains(&PendingRoleV1::PendingCommit))
                .count()
                != 1
        {
            return Err("successor original pending commit private package missing".into());
        }
        let h = &self.origin.handoff;
        if source.scope != h.target().scope
            || source.handoff_position != h.handoff_position()
            || source.target_digest != h.target_digest()
            || source.activation_digest != h.activation_digest()
            || source.handoff_snapshot_root != h.raw_snapshot_root()
        {
            return Err("successor private source differs from original H".into());
        }
        if sequence
            != previous_usage
                .segments
                .checked_add(1)
                .ok_or("successor segment overflow")?
        {
            return Err("successor private sequence/usage mismatch".into());
        }
        previous_usage.validate().map_err(err)?;
        if previous_usage
            != public_state
                .successor_window
                .as_ref()
                .map(|s| s.usage)
                .unwrap_or_default()
        {
            return Err("successor export cannot reset original protected archive usage".into());
        }
        let public_bytes = encode_protocol(public_state, MAX_CLOSURE)?;
        let mut objects = BTreeMap::<String, BftSuccessorWindowArchiveRefV1>::new();
        let mut total = 0u64;
        if let Some((protected, index_ref)) = protected {
            for reference in protected.objects.values().chain(std::iter::once(index_ref)) {
                reference.validate().map_err(err)?;
                if let Some(old) = objects.insert(reference.digest.clone(), reference.clone()) {
                    if old != *reference {
                        return Err("protected archive duplicate object length differs".into());
                    }
                } else {
                    total = total
                        .checked_add(reference.bytes)
                        .filter(|n| *n <= MAX_CLOSURE as u64)
                        .ok_or("protected archive segment capacity")?;
                }
            }
        }
        let (metadata, metadata_ref, public_state, cut_body) = {
            let mut put =
                |bytes: &[u8], max: usize| -> Result<BftSuccessorWindowArchiveRefV1, String> {
                    live(deadline)?;
                    let digest = blake3_hex(bytes);
                    if let Some(reference) = objects.get(&digest) {
                        return Ok(reference.clone());
                    }
                    if objects.len() >= MAX_REFS {
                        return Err("successor private object count".into());
                    }
                    total = total
                        .checked_add(bytes.len() as u64)
                        .filter(|n| *n <= MAX_CLOSURE as u64)
                        .ok_or("successor private archive capacity")?;
                    let reference = archive.persist_object(bytes, max).map_err(err)?;
                    if archive.load_object(&reference, max).map_err(err)? != bytes {
                        return Err("successor private fresh read differs".into());
                    }
                    live(deadline)?;
                    objects.insert(digest, reference.clone());
                    Ok(reference)
                };
            let current_journal = public_state
                .journal
                .as_ref()
                .map(|journal| put(&encode_protocol(journal, MAX_CLOSURE)?, MAX_CLOSURE))
                .transpose()?;
            let public_state = put(&public_bytes, MAX_CLOSURE)?;
            let material = put(&encoded_h_material(&self.inputs.material)?, MAX_H_BYTES)?;
            let origin = put(&encode(self.ledger.origin()?, MAX_METADATA)?, MAX_METADATA)?;
            let records = self
                .ledger
                .records_through(height)?
                .map(|r| {
                    put(
                        &encode(r, super::super::super::formal_ledger::MAX_RECORD)?,
                        super::super::super::formal_ledger::MAX_RECORD,
                    )
                })
                .collect::<Result<Vec<_>, String>>()?;
            let nodes = self
                .origin
                .retained_nodes
                .accessed_nodes()?
                .into_values()
                .map(|bytes| {
                    put(
                        &bytes,
                        super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                    )
                })
                .collect::<Result<Vec<_>, String>>()?;
            let mut nodes = nodes;
            nodes.sort_by(|a, b| a.digest.cmp(&b.digest));
            let seed = self
                .origin
                .retained_nodes
                .accessed_nodes()?
                .into_iter()
                .collect();
            let mut replay = FormalBftHeaderExecutionV3::from_handoff(
                self.inputs.anchor.clone(),
                self.inputs.target.clone(),
                &self.inputs.material,
                self.inputs.source.clone(),
                seed,
                true,
            )?;
            let mut snapshots = Vec::new();
            let mut journals = Vec::new();
            let mut manifests = Vec::new();
            for record in self.ledger.records_through(height)? {
                live(deadline)?;
                let entry = record.input();
                let executed = replay.replay_next(&entry.certificate, &entry.actions_cbor)?;
                let snapshot = put(&executed.snapshot_bytes, MAX_CLOSURE)?;
                let journal = put(&executed.journal_bytes, MAX_CLOSURE)?;
                if snapshot.digest != record.snapshot_ref()
                    || journal.digest != record.journal_ref()
                {
                    return Err(
                        "successor reexecution differs from original Snapshot/Journal".into(),
                    );
                }
                manifests.push(put(
                    &encode(executed.world.manifest(), MAX_CLOSURE)?,
                    MAX_CLOSURE,
                )?);
                snapshots.push(snapshot);
                journals.push(journal);
            }
            let snapshot_ref = snapshots.last().ok_or("successor empty cut")?.clone();
            let journal_ref = journals.last().ok_or("successor empty cut")?.clone();
            let certificate = cut.proof().certificate().certified_header();
            let c = &certificate.execution_commitment;
            let cut_body = BftSuccessorWindowCutV1 {
                height,
                header_id: header_id.into(),
                certificate_digest: blake3_hex(&certificate.canonical_bytes()?),
                execution_commitment_digest: c.digest()?,
                snapshot_ref,
                journal_ref,
                semantic_state_root: c.state_root.clone(),
                post_manifest_hash: c.post_manifest_hash.clone(),
                business_result_root: c.result_root.clone(),
                driver_record_digest: cut.durable_record_digest().into(),
                driver_install_digest: install_digest(
                    &source.material_identity,
                    cut.durable_record_digest(),
                    certificate,
                )?,
            };
            if cut_body.snapshot_ref.digest != c.execution_state_root {
                return Err("successor private Snapshot differs from committed original".into());
            }
            let mut pending_candidates = Vec::new();
            for (roles, package) in pending_packages {
                let mut dependencies = Vec::new();
                for (digest, bytes) in &package.dependency_nodes {
                    let reference = put(
                        bytes,
                        super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                    )?;
                    if reference.digest != *digest {
                        return Err("successor pending index identity".into());
                    }
                    dependencies.push(reference);
                }
                pending_candidates.push(PendingCandidateV1 {
                    height: height
                        .checked_add(1)
                        .ok_or("successor pending height overflow")?,
                    header_id: package.candidate.public_value.block_id()?,
                    roles: roles.clone(),
                    value: package.candidate.public_value.clone(),
                    actions: put(
                        &package.candidate.actions_cbor,
                        oasis7_proto::distributed_bft::BFT_MAX_ACTIONS_CBOR_BYTES_V1,
                    )?,
                    dependencies,
                });
            }
            pending_candidates.sort_by(|a, b| a.header_id.cmp(&b.header_id));
            let mut publications = Vec::new();
            for raw in publication_materials {
                publications.push(ArchivedPublicationV1 {
                    data: put(&raw.data, super::super::super::formal_ledger::MAX_RECORD)?,
                    original_data: raw
                        .original_data
                        .as_ref()
                        .map(|bytes| put(bytes, super::super::super::formal_ledger::MAX_RECORD))
                        .transpose()?,
                    reservation: put(
                        &raw.reservation,
                        super::super::super::formal_ledger::MAX_PUBLICATION_METADATA_BYTES,
                    )?,
                });
            }
            publications.sort_by(|a, b| a.data.digest.cmp(&b.data.digest));
            let metadata = PrivateClosureV1 {
                domain: if protected.is_some() {
                    protected_retention::PROTECTED_DOMAIN
                } else {
                    PUBLICATION_DOMAIN
                }
                .into(),
                publication_layout: if protected.is_some() {
                    Some(2)
                } else {
                    Some(1)
                },
                active_base: None,
                source: source.clone(),
                private_instance_id: private_instance_id.into(),
                material,
                origin,
                public_state: public_state.clone(),
                current_journal,
                pending_candidates,
                publications,
                snapshots,
                journals,
                manifests,
                records,
                nodes,
                protected_retention: protected.map(|(_, reference)| reference.clone()),
                cut: cut_body.clone(),
            };
            metadata.validate()?;
            let metadata_ref = put(&encode(&metadata, MAX_METADATA)?, MAX_METADATA)?;
            (metadata, metadata_ref, public_state, cut_body)
        };
        let (init_bytes, init_objects) =
            carry_initialization::from_raw(&metadata, publication_materials)?;
        let (wal_bytes, wal_objects) = publication_layout_reserve(metadata.publication_layout)?;
        let init_bytes = init_bytes
            .checked_add(wal_bytes)
            .ok_or("publication initializer reserve overflow")?;
        let init_objects = init_objects
            .checked_add(wal_objects)
            .ok_or("publication initializer object reserve overflow")?;
        let segment_bytes = total
            .checked_add(init_bytes)
            .and_then(|n| n.checked_add(MAX_METADATA as u64))
            .filter(|n| *n <= MAX_CLOSURE as u64)
            .ok_or("successor private segment capacity including public manifest reserve")?;
        let base_bytes = previous_usage
            .total_bytes
            .checked_add(segment_bytes)
            // Persistently charge one bounded public manifest too. This is a
            // conservative reserve, not an exact physical-byte measurement.
            .ok_or("successor archive usage overflow")?;
        let object_count = previous_usage
            .objects
            .checked_add(objects.len() as u64)
            .and_then(|n| n.checked_add(2 + init_objects)) // manifests and permanently reserved initialization objects
            .ok_or("successor object usage overflow")?;
        let mut manifest = BftSuccessorWindowArchiveManifestV1 {
            public_index: None,
            schema_version: 1,
            source: source.clone(),
            kind: BftSuccessorWindowArchiveKindV1::Execution,
            segment_sequence: sequence,
            predecessor_root,
            first_height: self.ledger.origin()?.first_height(),
            last_height: height,
            entries: objects
                .into_values()
                .map(|object| BftSuccessorWindowArchiveEntryV1 {
                    identity_digest: if object == metadata_ref {
                        metadata_identity()
                    } else {
                        object.digest.clone()
                    },
                    object,
                })
                .collect(),
            usage: BftSuccessorWindowArchiveUsageV1 {
                segments: sequence,
                objects: object_count,
                total_bytes: base_bytes,
            },
        };
        manifest
            .entries
            .sort_by(|a, b| a.identity_digest.cmp(&b.identity_digest));
        // Only usage.total_bytes depends on the manifest's own encoded length.
        // CBOR integer lengths grow monotonically; require an actual fixed point.
        let mut stable = false;
        for _ in 0..16 {
            let length = encode_protocol(&manifest, MAX_METADATA)?.len() as u64;
            let bytes = base_bytes
                .checked_add(length)
                .ok_or("successor manifest usage overflow")?;
            if bytes == manifest.usage.total_bytes {
                stable = true;
                break;
            }
            manifest.usage.total_bytes = bytes;
        }
        if !stable {
            return Err("successor manifest accounting did not converge".into());
        }
        manifest.validate().map_err(err)?;
        let bytes = encode_protocol(&manifest, MAX_METADATA)?;
        if segment_bytes
            .checked_add(bytes.len() as u64)
            .is_none_or(|n| n > MAX_CLOSURE as u64)
        {
            return Err("successor private segment exceeds capacity with both manifests".into());
        }
        let manifest_ref = archive.persist_object(&bytes, MAX_METADATA).map_err(err)?;
        if archive
            .load_object(&manifest_ref, MAX_METADATA)
            .map_err(err)?
            != bytes
        {
            return Err("successor manifest fresh read differs".into());
        }
        live(deadline)?;
        Ok(ExportedBftSuccessorPrivateClosureV1 {
            manifest,
            manifest_ref,
            metadata_ref,
            public_state_ref: public_state,
            pending_actions: metadata
                .pending_candidates
                .iter()
                .map(|c| c.actions.clone())
                .collect(),
            cut: cut_body,
        })
    }
}

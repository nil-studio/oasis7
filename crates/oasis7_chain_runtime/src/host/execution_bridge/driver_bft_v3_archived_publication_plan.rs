//! Size planning from actual execution. Placeholder signatures are encoded
//! only to bound storage; they are never signing or publication capabilities.
use super::super::super::super::super::super::super::storage as common_storage;
use super::super::super::super::super::formal_ledger;
use super::publication_metadata::MAX_PUBLICATION_METADATA_BYTES;

use super::*;
use oasis7_formal_protocol::bft::{
    BFT_FORMAL_HEADER_PREFIX_MAX_HEIGHTS_V3, FormalBftCertifiedHeaderPrefixV3,
};
use oasis7_proto::distributed_bft::v3::{
    BFT_PROTOCOL_VERSION_V3, BftHeaderCommitCertificateV3, BftProposalHeaderV3,
    BftValidRoundEvidenceV3, BftVotePhaseV3, BftVoteV3, BftVoteValueV3,
};
use serde::Serialize;
// This is the exact successor-window record domain from the fixed producer.
const WINDOW_RECORD_DOMAIN: &str = "oasis7.formal.bft.private-window.record.v1";

#[derive(Serialize)]
struct RecordEnvelope<'a> {
    schema_version: u32,
    domain: &'a str,
    origin_digest: &'a str,
    parent_record_digest: &'a str,
    height: u64,
    certificate: &'a FormalBftCertifiedHeaderV3,
    actions_cbor: &'a [u8],
    snapshot_ref: &'a str,
    journal_ref: &'a str,
    business_index_refs: Vec<&'a str>,
    publication_reservation_ref: Option<&'a str>,
}
pub(super) fn planned_publication_bytes(
    schedule: &BftHeaderHeightScheduleV3,
    executed: &ExecutedFormalHeaderV3,
    actions: &[u8],
    nodes: &std::collections::BTreeMap<String, usize>,
    prefix: &[FormalBftCertifiedHeaderV3],
) -> Result<usize, String> {
    if prefix.len() >= BFT_FORMAL_HEADER_PREFIX_MAX_HEIGHTS_V3 {
        return Err("publication plan has no current record slot".into());
    }
    if executed.snapshot_bytes.len() > common_storage::MAX_BFT_SNAPSHOT_BYTES
        || executed.journal_bytes.len() > common_storage::MAX_BFT_JOURNAL_BYTES
    {
        return Err("publication Snapshot or Journal exceeds existing object bound".into());
    }
    let id = executed.candidate.header.block_id()?;
    let validators = schedule.active_set().validators();
    let longest = validators
        .values()
        .max_by_key(|v| v.validator_id.len())
        .ok_or("publication plan has no active validators")?;
    let proposal = |round, valid_round| BftProposalHeaderV3 {
        protocol_version: BFT_PROTOCOL_VERSION_V3,
        scope: schedule.scope().clone(),
        height: schedule.height(),
        round,
        active_validator_set_hash: schedule.active_set().set_hash().into(),
        schedule_policy_hash: schedule.policy_hash().into(),
        proposer_id: longest.validator_id.clone(),
        block_id: id.clone(),
        valid_round,
        public_key_hex: longest.public_key_hex.clone(),
        signature_hex: "00".repeat(64),
    };
    let votes = |phase, round| {
        validators
            .values()
            .map(|validator| BftVoteV3 {
                protocol_version: BFT_PROTOCOL_VERSION_V3,
                scope: schedule.scope().clone(),
                height: schedule.height(),
                round,
                phase,
                active_validator_set_hash: schedule.active_set().set_hash().into(),
                schedule_policy_hash: schedule.policy_hash().into(),
                value: BftVoteValueV3::Block {
                    block_id: id.clone(),
                },
                validator_id: validator.validator_id.clone(),
                public_key_hex: validator.public_key_hex.clone(),
                signature_hex: "00".repeat(64),
            })
            .collect()
    };
    let qc = FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal: proposal(u64::MAX, Some(u64::MAX - 1)),
            valid_round_evidence: Some(BftValidRoundEvidenceV3 {
                proposal: proposal(u64::MAX - 1, Some(u64::MAX - 2)),
                prevotes: votes(BftVotePhaseV3::Prevote, u64::MAX - 1),
            }),
            precommits: votes(BftVotePhaseV3::Precommit, u64::MAX),
        },
        executed.candidate.header.clone(),
        executed.candidate.commitment.clone(),
    )?;
    let placeholder = "00".repeat(32);
    // Candidate dependency objects have already passed exact byte/hash
    // validation. The union retains an exact length per content digest.
    let mut references = nodes
        .iter()
        .map(|(digest, bytes)| (digest.as_str(), *bytes))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut derived_bytes = 0usize;
    for write in &executed.result_delta.node_writes {
        formal_dependencies::validate_node(&write.digest, &write.bytes)?;
        let length = write.bytes.len();
        if references
            .insert(write.digest.as_str(), length)
            .is_some_and(|old| old != length)
        {
            return Err("publication derived node bytes conflict".into());
        }
        derived_bytes = derived_bytes
            .checked_add(length)
            .ok_or("publication derived byte overflow")?;
    }
    if references.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES {
        return Err("publication derived node capacity".into());
    }
    if derived_bytes > formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES {
        return Err("publication derived allocation byte capacity".into());
    }
    let union_bytes = references.values().try_fold(0usize, |total, bytes| {
        total
            .checked_add(*bytes)
            .filter(|n| *n <= formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES)
            .ok_or("publication union index byte capacity")
    })?;
    // One original T+1 promise may be consumed after an authenticated
    // window migration. Encode both actual Record domains; no padding or
    // caller-selected domain can understate the successor envelope.
    let references = references.into_keys().collect::<Vec<_>>();
    let mut record_upper_bytes = 0usize;
    for domain in [storage::RECORD_DOMAIN, WINDOW_RECORD_DOMAIN] {
        record_upper_bytes = record_upper_bytes.max(
            serde_cbor::to_vec(&RecordEnvelope {
                schema_version: 1,
                domain,
                origin_digest: &placeholder,
                parent_record_digest: &placeholder,
                height: schedule.height(),
                certificate: &qc,
                actions_cbor: actions,
                snapshot_ref: &placeholder,
                journal_ref: &placeholder,
                business_index_refs: references.clone(),
                publication_reservation_ref: Some(&placeholder),
            })
            .map_err(|_| "encode publication Record bound")?
            .len(),
        );
    }
    if record_upper_bytes > formal_ledger::MAX_RECORD {
        return Err("publication single record capacity".into());
    }
    let mut entries = prefix.to_vec();
    entries.push(qc);
    let prefix_upper_bytes = FormalBftCertifiedHeaderPrefixV3 {
        schema_version: 1,
        entries,
    }
    .canonical_bytes()?
    .len();
    let publication_bytes = record_upper_bytes
        .checked_mul(4)
        .and_then(|n| n.checked_add(prefix_upper_bytes))
        .and_then(|n| n.checked_add(executed.snapshot_bytes.len()))
        .and_then(|n| n.checked_add(executed.journal_bytes.len()))
        .and_then(|n| n.checked_add(union_bytes))
        .and_then(|n| n.checked_add(MAX_PUBLICATION_METADATA_BYTES))
        .filter(|n| *n <= formal_ledger::MAX_MATERIAL)
        .ok_or("publication single operation material capacity")?;
    Ok(publication_bytes)
}

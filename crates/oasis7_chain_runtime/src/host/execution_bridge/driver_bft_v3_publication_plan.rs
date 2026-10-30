//! Size planning from actual execution. Placeholder signatures are encoded
//! only to bound storage; they are never signing or publication capabilities.
use super::super::super::super::storage as common_storage;
use super::*;
use oasis7_formal_protocol::bft::FormalBftCertifiedHeaderPrefixV3;
use oasis7_proto::distributed_bft::v3::{
    BFT_PROTOCOL_VERSION_V3, BftHeaderCommitCertificateV3, BftProposalHeaderV3,
    BftValidRoundEvidenceV3, BftVotePhaseV3, BftVoteV3, BftVoteValueV3,
};
use serde::Serialize;
#[derive(Clone)]
pub(super) struct PublicationPlanV1 {
    pub(super) candidate: FormalBftHeaderCandidateV3,
    pub(super) snapshot_digest: String,
    pub(super) snapshot_bytes: u64,
    pub(super) journal_digest: String,
    pub(super) journal_bytes: u64,
    pub(super) derived_nodes_digest: String,
    pub(super) record_upper_bytes: usize,
    pub(super) prefix_upper_bytes: usize,
    pub(super) publication_bytes: usize,
    #[cfg(test)]
    pub(super) encoded_qc: FormalBftCertifiedHeaderV3,
}
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
impl PublicationPlanV1 {
    pub(super) fn from_actual(
        schedule: &BftHeaderHeightScheduleV3,
        executed: &ExecutedFormalHeaderV3,
        actions: &[u8],
        nodes: &std::collections::BTreeMap<String, Vec<u8>>,
        prefix: &[FormalBftCertifiedHeaderV3],
        record_domain: &str,
    ) -> Result<Self, String> {
        if prefix.len() >= 64 {
            return Err("publication plan has no current record slot".into());
        }
        formal_dependencies::validate_nodes(nodes)?;
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
        let mut references = nodes
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let mut derived_bytes = 0usize;
        for write in &executed.result_delta.node_writes {
            formal_dependencies::validate_node(&write.digest, &write.bytes)?;
            if nodes
                .get(&write.digest)
                .is_some_and(|original| original != &write.bytes)
            {
                return Err("publication derived node bytes conflict".into());
            }
            references.insert(&write.digest);
            // Reserve all actually derived bytes, including conservative reuse;
            // actual transfer later follows ledger object deduplication.
            derived_bytes = derived_bytes
                .checked_add(write.bytes.len())
                .ok_or("publication derived byte overflow")?;
        }
        if references.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES {
            return Err("publication derived node capacity".into());
        }
        if derived_bytes > formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES {
            return Err("publication derived allocation byte capacity".into());
        }
        let mut union_bytes = 0usize;
        for reference in &references {
            let bytes = nodes
                .get(*reference)
                .map(Vec::as_slice)
                .or_else(|| {
                    executed
                        .result_delta
                        .node_writes
                        .iter()
                        .find(|write| write.digest.as_str() == *reference)
                        .map(|write| write.bytes.as_slice())
                })
                .ok_or("publication union node missing")?;
            union_bytes = union_bytes
                .checked_add(bytes.len())
                .filter(|total| *total <= formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES)
                .ok_or("publication union index byte capacity")?;
        }
        let record_upper_bytes = serde_cbor::to_vec(&RecordEnvelope {
            schema_version: 1,
            domain: record_domain,
            origin_digest: &placeholder,
            parent_record_digest: &placeholder,
            height: schedule.height(),
            certificate: &qc,
            actions_cbor: actions,
            snapshot_ref: &placeholder,
            journal_ref: &placeholder,
            business_index_refs: references.into_iter().collect(),
            publication_reservation_ref: Some(&placeholder),
        })
        .map_err(|_| "encode publication record bound")?
        .len();
        if record_upper_bytes > super::super::formal_ledger::MAX_RECORD {
            return Err("publication single record capacity".into());
        }
        let prefix_upper_bytes = encoded_prefix_upper_bytes(prefix, &qc)?;
        let publication_bytes = record_upper_bytes
            .checked_mul(4)
            .and_then(|n| n.checked_add(prefix_upper_bytes))
            .and_then(|n| n.checked_add(executed.snapshot_bytes.len()))
            .and_then(|n| n.checked_add(executed.journal_bytes.len()))
            .and_then(|n| n.checked_add(union_bytes))
            .and_then(|n| {
                n.checked_add(super::super::formal_ledger::MAX_PUBLICATION_METADATA_BYTES)
            })
            .filter(|n| *n <= super::super::formal_ledger::MAX_MATERIAL)
            .ok_or("publication single operation material capacity")?;
        Ok(Self {
            candidate: executed.candidate.clone(),
            snapshot_digest: blake3_hex(&executed.snapshot_bytes),
            snapshot_bytes: executed.snapshot_bytes.len() as u64,
            journal_digest: blake3_hex(&executed.journal_bytes),
            journal_bytes: executed.journal_bytes.len() as u64,
            derived_nodes_digest: blake3_hex(
                &serde_cbor::to_vec(&executed.result_delta.node_writes)
                    .map_err(|_| "encode publication delta identity")?,
            ),
            record_upper_bytes,
            prefix_upper_bytes,
            publication_bytes,
            #[cfg(test)]
            encoded_qc: qc,
        })
    }
}

/// Exact codec/bound shared by the actual planner and codec stress tests.
/// It does not verify a history or create an execution publication capability.
pub(super) fn encoded_prefix_upper_bytes(
    prefix: &[FormalBftCertifiedHeaderV3],
    next: &FormalBftCertifiedHeaderV3,
) -> Result<usize, String> {
    let mut entries = prefix.to_vec();
    entries.push(next.clone());
    Ok(FormalBftCertifiedHeaderPrefixV3 {
        schema_version: 1,
        entries,
    }
    .canonical_bytes()?
    .len())
}

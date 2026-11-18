//! Read-only original candidate execution against an independently qualified archive parent.
use super::*;
use oasis7_consensus::tendermint_v3::BftHeaderValueV3;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputRefV1, BftPrivateRetentionInputObjectKindV1 as Kind,
};
use oasis7_node::NodeBftPrivateCandidateV3;

#[path = "driver_bft_v3_archived_publication_metadata.rs"]
mod publication_metadata;
use publication_metadata::PublicationReservationV1;
#[path = "driver_bft_v3_archived_publication_plan.rs"]
mod publication_plan;
use publication_plan::planned_publication_bytes;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OriginalCandidateArchivePinV1 {
    reference: BftPrivateRetentionAdmissionInputRefV1,
    original_private_origin_digest: String,
    original_instance_id: String,
}

#[derive(Serialize)]
pub(crate) struct ReplayedOriginalCandidateV1 {
    height: u64,
    header_id: String,
    data_schema: u32,
    publication_bytes: u64,
    semantic_state_root: String,
    result_root: String,
}

// The protected candidate subset of the fixed Data schema. Its field order is
// part of the original CBOR contract. Origin/selection/uncovered slots differ.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OriginalCandidateData {
    schema_version: u32,
    domain: String,
    origin: bool,
    ledger_origin_digest: String,
    material_identity: String,
    instance_id: String,
    height: u64,
    value: Option<BftHeaderValueV3>,
    #[serde(deserialize_with = "super::super::super::private::deserialize_actions")]
    actions_cbor: Vec<u8>,
    #[serde(deserialize_with = "super::super::super::private::deserialize_candidate_dependencies")]
    references: Vec<String>,
    publication_reservation_ref: String,
    protected_source_origin_ref: String,
}

impl OriginalCandidateArchivePinV1 {
    pub(crate) fn replay(
        &self,
        archive: &LocalBftSuccessorArchiveStoreV1,
        reference: &CommittedPrefixArchiveRefV1,
        source: &BftSuccessorWindowSourceV1,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        expected_cut: (u64, &str),
    ) -> Result<(ReplayedCommittedPrefixV1, ReplayedOriginalCandidateV1), String> {
        let (cut, parent) = FormalBftHeaderLedgerExecutionV3::replay_closed_prefix(
            archive,
            reference,
            source,
            anchor,
            target,
            expected_cut.0,
            expected_cut.1,
        )?;
        let candidate = self.replay_candidate(archive, source, &parent)?;
        Ok((cut, candidate))
    }

    fn replay_candidate(
        &self,
        archive: &LocalBftSuccessorArchiveStoreV1,
        source: &BftSuccessorWindowSourceV1,
        parent: &ClosedCandidateParent,
    ) -> Result<ReplayedOriginalCandidateV1, String> {
        let origin = parent.ledger_origin.as_str();
        let execution = &parent.execution;
        if self.reference.source.binding_digest != source.binding_digest
            || self.reference.source.handoff_digest != source.handoff_record_id
            || self.reference.source.material_source_digest != source.material_identity
            || self.reference.key.ledger_origin_digest != origin
            || self.reference.key.height != execution.schedule.height()
        {
            return Err(
                "original candidate reference differs from the exact archive parent".into(),
            );
        }
        for digest in [
            &self.original_private_origin_digest,
            &self.original_instance_id,
        ] {
            BftSuccessorWindowArchiveRefV1 {
                digest: digest.clone(),
                bytes: 1,
            }
            .validate()
            .map_err(error)?;
        }
        let input = archive
            .load_original_admission_input(&self.reference)
            .map_err(error)?;
        let mut data = None;
        let mut metadata = None;
        let mut dependencies = Vec::new();
        for object in input.objects {
            match object.kind {
                Kind::OriginalData => data = Some(object.bytes),
                Kind::PublicationMetadata => metadata = Some(object),
                Kind::Dependency => dependencies.push((object.reference.digest, object.bytes)),
            }
        }
        let bytes = data.ok_or("original candidate Data absent")?;
        let data: OriginalCandidateData =
            serde_cbor::from_slice(&bytes).map_err(|_| "decode protected candidate Data")?;
        if !matches!(
            (data.schema_version, data.domain.as_str()),
            (9, "oasis7.formal.bft.node_private_protected_input.v1")
                | (
                    10,
                    "oasis7.formal.bft.node_private_protected_received_input.v1"
                )
                | (11, "oasis7.formal.bft.node_private_active_input.v11")
                | (
                    12,
                    "oasis7.formal.bft.node_private_active_received_input.v12"
                )
        ) || data.origin
            || data.protected_source_origin_ref != self.original_private_origin_digest
            || data.instance_id != self.original_instance_id
            || data.ledger_origin_digest != origin
            || data.material_identity != source.material_identity
            || data.height != self.reference.key.height
            || metadata.as_ref().map(|object| &object.reference.digest)
                != Some(&data.publication_reservation_ref)
            || data
                .references
                .iter()
                .ne(dependencies.iter().map(|(digest, _)| digest))
            || serde_cbor::to_vec(&data).map_err(error)? != bytes
        {
            return Err("protected candidate Data differs from pinned original material".into());
        }
        let metadata = PublicationReservationV1::decode(
            &metadata
                .ok_or("original publication metadata absent")?
                .bytes,
        )?;
        let candidate = NodeBftPrivateCandidateV3 {
            public_value: data.value.ok_or("original candidate public value absent")?,
            actions_cbor: data.actions_cbor,
        };
        if candidate.public_value.block_id()? != self.reference.key.header_id {
            return Err("original candidate value differs from its exact header".into());
        }
        let dependency_sizes = dependencies
            .iter()
            .map(|(digest, bytes)| (digest.clone(), bytes.len()))
            .collect();
        let executed = execution.execute_closed_candidate(&candidate.actions_cbor, dependencies)?;
        if executed.candidate.header != candidate.public_value.header
            || executed.candidate.commitment != candidate.public_value.execution_commitment
        {
            return Err("original candidate differs from exact parent execution".into());
        }
        metadata.verify_execution(
            origin,
            &data.material_identity,
            &data.instance_id,
            &candidate.actions_cbor,
            &executed,
        )?;
        let publication_bytes = planned_publication_bytes(
            &execution.schedule,
            &executed,
            &candidate.actions_cbor,
            &dependency_sizes,
            &parent.certified.entries,
        )?;
        metadata.verify_publication_bytes(publication_bytes)?;
        Ok(ReplayedOriginalCandidateV1 {
            height: data.height,
            header_id: self.reference.key.header_id.clone(),
            data_schema: data.schema_version,
            publication_bytes: publication_bytes as u64,
            semantic_state_root: executed.candidate.commitment.state_root,
            result_root: executed.candidate.commitment.result_root,
        })
    }
}

#[cfg(test)]
impl FormalBftHeaderLedgerExecutionV3 {
    // Fixture uses real World output, with an independently fixed operator pin
    // and synthetically encoded covered Data. No Reserve or issuer is simulated.
    pub(crate) fn fixture_archived_candidate(
        &self,
        archive: &LocalBftSuccessorArchiveStoreV1,
        source: &BftSuccessorWindowSourceV1,
        actions_cbor: Vec<u8>,
        mutation: Option<&str>,
    ) -> (serde_json::Value, String, String) {
        use oasis7_distfs::protected_bft_v3::{
            BftPrivateRetentionAdmissionInputV1, BftPrivateRetentionInputObjectV1,
            BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1,
        };

        let record = |kind, bytes: Vec<u8>| BftPrivateRetentionInputObjectV1 {
            kind,
            reference: BftSuccessorWindowArchiveRefV1 {
                digest: blake3_hex(&bytes),
                bytes: bytes.len() as u64,
            },
            bytes,
        };
        let archive_only = Arc::new(ArchiveOnlyReader);
        let recorder = formal_dependencies::RecordingBusinessIndexReaderV1::new(
            archive_only,
            self.origin.retained_nodes.recorded_nodes().unwrap(),
        )
        .unwrap();
        let original = self.origin.with_candidate_reader(recorder.clone()).unwrap();
        let executed = original.execute(&actions_cbor).unwrap();
        let candidate = &executed.candidate;
        let state_root = candidate.commitment.state_root.clone();
        let header_id = candidate.header.block_id().unwrap();
        let mut nodes = recorder.accessed_nodes().unwrap();
        let certified = self
            .ledger
            .records()
            .filter(|record| record.height() < original.schedule.height())
            .map(|record| record.input().certificate)
            .collect::<Vec<_>>();
        let planned = planned_publication_bytes(
            &original.schedule,
            &executed,
            &actions_cbor,
            &nodes
                .iter()
                .map(|(id, bytes)| (id.clone(), bytes.len()))
                .collect(),
            &certified,
        )
        .unwrap();
        match mutation {
            Some("missing") => {
                assert!(nodes.pop_first().is_some());
            }
            Some("extra") => {
                let extra = b"unobserved archived node".to_vec();
                nodes.insert(blake3_hex(&extra), extra);
            }
            Some("snapshot" | "journal" | "derived" | "publication") => {}
            Some(other) => panic!("unsupported fixture mutation {other}"),
            None => {}
        }
        let origin = self.ledger.origin().unwrap().digest().unwrap();
        let metadata = record(
            Kind::PublicationMetadata,
            PublicationReservationV1::fixture(
                &origin,
                &source.material_identity,
                &"11".repeat(32),
                &actions_cbor,
                &executed,
                planned,
                mutation,
            ),
        );
        let data = OriginalCandidateData {
            schema_version: 11,
            domain: "oasis7.formal.bft.node_private_active_input.v11".into(),
            origin: false,
            ledger_origin_digest: origin.clone(),
            material_identity: source.material_identity.clone(),
            instance_id: "11".repeat(32),
            height: candidate.commitment.height,
            value: Some(BftHeaderValueV3 {
                header: candidate.header.clone(),
                execution_commitment: candidate.commitment.clone(),
            }),
            actions_cbor,
            references: nodes.keys().cloned().collect(),
            publication_reservation_ref: metadata.reference.digest.clone(),
            protected_source_origin_ref: "22".repeat(32),
        };
        let mut objects = vec![
            record(Kind::OriginalData, serde_cbor::to_vec(&data).unwrap()),
            metadata,
        ];
        objects.extend(
            nodes
                .into_values()
                .map(|bytes| record(Kind::Dependency, bytes)),
        );
        objects.sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
        let admission = BftPrivateRetentionAdmissionInputV1 {
            schema_version: 1,
            source: BftPrivateRetentionSourceV1 {
                binding_digest: source.binding_digest.clone(),
                handoff_digest: source.handoff_record_id.clone(),
                material_source_digest: source.material_identity.clone(),
            },
            key: BftPrivateRetentionKeyV1 {
                ledger_origin_digest: origin,
                height: data.height,
                header_id: header_id.clone(),
            },
            publication_reservation_bytes: 123,
            objects,
        };
        let reference = admission.descriptor().unwrap();
        archive
            .persist_original_admission_input(&reference, &admission)
            .unwrap();
        let pin = serde_json::json!({
            "reference": reference,
            "original_private_origin_digest": data.protected_source_origin_ref,
            "original_instance_id": data.instance_id,
        });
        (pin, header_id, state_root)
    }
}

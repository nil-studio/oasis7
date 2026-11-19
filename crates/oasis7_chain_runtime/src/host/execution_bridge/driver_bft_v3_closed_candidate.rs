//! Read-only original candidate execution against an independently qualified archive parent.
use super::*;
use oasis7_consensus::tendermint_v3::BftHeaderValueV3;
use oasis7_distfs::protected_bft_v3::{
    BftFirstReservationPriceV1, BftPrivateRetentionAdmissionInputRefV1,
    BftPrivateRetentionAdmissionInputV1, BftPrivateRetentionCreationMarkerV1,
    BftPrivateRetentionInputObjectKindV1 as Kind,
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
    pub(crate) reference: BftPrivateRetentionAdmissionInputRefV1,
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
        let input = archive
            .load_original_admission_input(&self.reference)
            .map_err(error)?;
        let candidate = self.replay_candidate(
            &input,
            source,
            &parent.execution,
            &parent.ledger_origin,
            &parent.certified.entries,
        )?;
        Ok((cut, candidate))
    }

    /// Calculate the original first charge from the independently qualified H.
    /// The local verifier receives its creation marker and private-origin pin
    /// from deployment authorization. Transport authorization is separate.
    pub(crate) fn verify_first_price(
        &self,
        ledger: &FormalBftHeaderLedgerExecutionV3,
        input: &BftPrivateRetentionAdmissionInputV1,
        source: &BftSuccessorWindowSourceV1,
        marker: &BftPrivateRetentionCreationMarkerV1,
        marker_digest: &str,
    ) -> Result<(ReplayedOriginalCandidateV1, BftFirstReservationPriceV1), String> {
        let qualified = &ledger.origin;
        source_at_h(source, &qualified.handoff)?;
        let origin = ledger.ledger.origin()?;
        let ledger_origin = origin.digest()?;
        let actual_material = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                &ledger_origin,
                &marker.creation_identity,
                origin.material_pin_digest(),
            ))
            .map_err(error)?,
        );
        if marker.digest().map_err(error)? != marker_digest
            || marker.source != self.reference.source
            || marker.creation_identity != self.original_instance_id
            || marker.ledger_origin_digest != ledger_origin
            || marker.activation_digest != qualified.handoff.activation_digest()
            || marker.initial_height != qualified.schedule.height()
            || actual_material != source.material_identity
        {
            return Err("first price differs from pinned original H source".into());
        }
        let execution = FormalBftHeaderExecutionV3::from_handoff(
            ledger.inputs.anchor.clone(),
            ledger.inputs.target.clone(),
            &ledger.inputs.material,
            ledger.inputs.source.clone(),
            Vec::new(),
            false,
        )?;
        super::super::node_execution::capture_h_index(&execution)?;
        let retained_h_dependencies = execution
            .retained_nodes
            .accessed_nodes()?
            .into_keys()
            .collect();
        let candidate = self.replay_candidate(input, source, &execution, &ledger_origin, &[])?;
        if !matches!(candidate.data_schema, 9 | 10) {
            return Err("first price requires original source Local or Received Data".into());
        }
        let price =
            first_price_bounds(input, candidate.publication_bytes, retained_h_dependencies)?;
        price.verify_input(input).map_err(error)?;
        Ok((candidate, price))
    }

    fn replay_candidate(
        &self,
        input: &BftPrivateRetentionAdmissionInputV1,
        source: &BftSuccessorWindowSourceV1,
        execution: &FormalBftHeaderExecutionV3,
        origin: &str,
        certified: &[oasis7_formal_protocol::bft::FormalBftCertifiedHeaderV3],
    ) -> Result<ReplayedOriginalCandidateV1, String> {
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
        self.reference.validate_input(input).map_err(error)?;
        let mut data = None;
        let mut metadata = None;
        let mut dependencies = Vec::new();
        for object in &input.objects {
            match object.kind {
                Kind::OriginalData => data = Some(&object.bytes),
                Kind::PublicationMetadata => metadata = Some(object),
                Kind::Dependency => {
                    dependencies.push((object.reference.digest.clone(), object.bytes.clone()));
                }
            }
        }
        let bytes = data.ok_or("original candidate Data absent")?;
        let data: OriginalCandidateData =
            serde_cbor::from_slice(bytes).map_err(|_| "decode protected candidate Data")?;
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
            || serde_cbor::to_vec(&data).map_err(error)? != *bytes
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
            certified,
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

fn first_price_bounds(
    input: &BftPrivateRetentionAdmissionInputV1,
    publication_bytes: u64,
    retained_h_dependencies: std::collections::BTreeSet<String>,
) -> Result<BftFirstReservationPriceV1, String> {
    let object = input
        .objects
        .iter()
        .find(|object| object.kind == Kind::OriginalData)
        .ok_or("first price original Data absent")?;
    let data: OriginalCandidateData = serde_cbor::from_slice(&object.bytes)
        .map_err(|_| "decode original Data for first price")?;

    fn pin_bytes(pins: std::collections::BTreeSet<&str>) -> Result<u64, String> {
        #[derive(Serialize)]
        struct Receipt<'a> {
            pins: std::collections::BTreeSet<&'a str>,
        }
        (serde_json::to_vec(&Receipt { pins }).map_err(error)?.len() as u64)
            .checked_mul(2)
            .ok_or("original pin receipt footprint overflow".into())
    }
    let private_pin_bytes = pin_bytes(
        data.references
            .iter()
            .map(String::as_str)
            .chain(
                [
                    &object.reference.digest,
                    &data.publication_reservation_ref,
                    &data.protected_source_origin_ref,
                ]
                .into_iter()
                .map(String::as_str),
            )
            .collect(),
    )?;

    let original_selection_bytes = if data.schema_version == 10 {
        // Same schema 5 and field order as the ordinary immutable local selection.
        #[derive(Serialize)]
        struct Selection<'a> {
            schema_version: u32,
            domain: &'static str,
            origin: bool,
            ledger_origin_digest: &'a str,
            material_identity: &'a str,
            instance_id: &'a str,
            height: u64,
            value: Option<&'a BftHeaderValueV3>,
            actions_cbor: Vec<u8>,
            references: [&'a str; 1],
        }
        let selection = serde_cbor::to_vec(&Selection {
            schema_version: 5,
            domain: "oasis7.formal.bft.node_private_selection.v5",
            origin: false,
            ledger_origin_digest: &data.ledger_origin_digest,
            material_identity: &data.material_identity,
            instance_id: &data.instance_id,
            height: data.height,
            value: data.value.as_ref(),
            actions_cbor: Vec::new(),
            references: [&object.reference.digest],
        })
        .map_err(error)?;
        let digest = blake3_hex(&selection);
        let pins = pin_bytes(
            [object.reference.digest.as_str(), &digest]
                .into_iter()
                .collect(),
        )?;
        (selection.len() as u64)
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(pins))
            .ok_or("original selection footprint overflow")?
    } else {
        0
    };

    Ok(BftFirstReservationPriceV1 {
        publication_bytes,
        private_pin_bytes,
        original_selection_bytes,
        retained_h_dependencies,
    })
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
            Some(
                "snapshot" | "journal" | "derived" | "publication" | "first-local"
                | "first-received" | "first-repriced",
            ) => {}
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
        let (schema_version, domain) = match mutation {
            Some("first-local" | "first-repriced") => {
                (9, "oasis7.formal.bft.node_private_protected_input.v1")
            }
            Some("first-received") => (
                10,
                "oasis7.formal.bft.node_private_protected_received_input.v1",
            ),
            _ => (11, "oasis7.formal.bft.node_private_active_input.v11"),
        };
        let data = OriginalCandidateData {
            schema_version,
            domain: domain.into(),
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
        let mut admission = BftPrivateRetentionAdmissionInputV1 {
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
        if matches!(schema_version, 9 | 10) {
            let retained: std::collections::BTreeSet<String> = self
                .origin
                .retained_nodes
                .accessed_nodes()
                .unwrap()
                .into_keys()
                .collect();
            let bounds = first_price_bounds(&admission, planned as u64, retained).unwrap();
            assert_eq!(bounds.original_selection_bytes > 0, schema_version == 10);
            let material = admission
                .objects
                .iter()
                .map(|object| match object.kind {
                    Kind::OriginalData => 2 * object.reference.bytes,
                    Kind::Dependency
                        if bounds
                            .retained_h_dependencies
                            .contains(&object.reference.digest) =>
                    {
                        0
                    }
                    _ => object.reference.bytes,
                })
                .sum::<u64>();
            admission.publication_reservation_bytes = bounds.publication_bytes
                + bounds.private_pin_bytes
                + bounds.original_selection_bytes
                + material
                + u64::from(mutation == Some("first-repriced"));
        }
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

#[cfg(test)]
impl FormalBftHeaderLedgerExecutionV3 {
    pub(crate) fn fixture_first_price_source(
        &self,
    ) -> (
        BftSuccessorWindowSourceV1,
        BftPrivateRetentionCreationMarkerV1,
    ) {
        let origin = self.ledger.origin().unwrap();
        let ledger_origin_digest = origin.digest().unwrap();
        let creation_identity = "11".repeat(32);
        let material_identity = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                &ledger_origin_digest,
                &creation_identity,
                origin.material_pin_digest(),
            ))
            .unwrap(),
        );
        let h = &self.origin.handoff;
        let source = BftSuccessorWindowSourceV1 {
            scope: h.target().scope.clone(),
            binding_digest: "12".repeat(32),
            handoff_position: h.handoff_position(),
            handoff_record_id: h.handoff_head().record_id.clone(),
            handoff_snapshot_root: h.raw_snapshot_root().into(),
            target_digest: h.target_digest().into(),
            activation_digest: h.activation_digest().into(),
            material_identity,
        };
        let marker = BftPrivateRetentionCreationMarkerV1 {
            schema_version: 1,
            creation_identity,
            source: oasis7_distfs::protected_bft_v3::BftPrivateRetentionSourceV1 {
                binding_digest: source.binding_digest.clone(),
                handoff_digest: source.handoff_record_id.clone(),
                material_source_digest: source.material_identity.clone(),
            },
            ledger_origin_digest,
            activation_digest: source.activation_digest.clone(),
            initial_state_digest: "31".repeat(32),
            initial_height: self.origin.schedule.height(),
        };
        super::super::node_execution::capture_h_index(&self.origin).unwrap();
        (source, marker)
    }
}

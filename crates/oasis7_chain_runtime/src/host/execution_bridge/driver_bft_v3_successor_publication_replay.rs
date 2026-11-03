//! Publication provenance is checked after actual H/archive replay, never from a raw package.
use super::*;
impl ActualBftSuccessorWindowDriverVerifierV1 {
    pub(super) fn load_publications(
        &self,
        metadata: &PrivateClosureV1,
        origin: &str,
        predecessor: Option<&VerifiedPrivateSuccessorBaseV1>,
        public_state: &BftSignerStateV3,
        deadline: Instant,
    ) -> Result<Vec<node_execution::VerifiedPublicationOriginalV1>, String> {
        let mut grants = Vec::new();
        let allowed: std::collections::BTreeSet<_> = metadata
            .pending_candidates
            .iter()
            .map(|p| p.header_id.clone())
            .chain(
                public_state
                    .history
                    .iter()
                    .map(|c| c.header.block_id())
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        for archived in &metadata.publications {
            let raw = node_execution::PublicationArchiveMaterialV1 {
                data: self.load(
                    &archived.data,
                    super::super::super::super::formal_ledger::MAX_RECORD,
                    deadline,
                )?,
                original_data: archived
                    .original_data
                    .as_ref()
                    .map(|r| {
                        self.load(
                            r,
                            super::super::super::super::formal_ledger::MAX_RECORD,
                            deadline,
                        )
                    })
                    .transpose()?,
                reservation: self.load(
                    &archived.reservation,
                    super::super::super::super::formal_ledger::MAX_PUBLICATION_METADATA_BYTES,
                    deadline,
                )?,
            };
            let grant = node_execution::VerifiedPublicationOriginalV1::verify(
                &raw,
                node_execution::PublicationArchiveOwnerV1 {
                    origin,
                    material: &metadata.source.material_identity,
                    instance: &metadata.private_instance_id,
                },
                predecessor
                    .map(|p| p.publications.as_slice())
                    .unwrap_or(&[]),
            )?;
            let id = grant.header_id()?;
            if !allowed.contains(&id) || !seen.insert(id) {
                return Err("archive publication is unproved or ambiguous".into());
            }
            grants.push(grant);
        }
        Ok(grants)
    }
    pub(super) fn validate_record_publication(
        &self,
        grants: &[node_execution::VerifiedPublicationOriginalV1],
        reference: Option<&str>,
        input: &PrivateHeaderEntryV3,
        executed: &ExecutedFormalHeaderV3,
    ) -> Result<(), String> {
        let id = input.certificate.header.block_id()?;
        let grant = grants
            .iter()
            .find(|g| g.header_id().as_deref() == Ok(id.as_str()));
        match (reference, grant) {
            (None, None) => Ok(()),
            (Some(reference), Some(grant)) if grant.reference()? == reference => {
                grant.validate_execution(&input.actions_cbor, executed)
            }
            _ => Err("archive Record publication ref differs from original Data".into()),
        }
    }
}

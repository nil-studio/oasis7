//! Narrow fixtures around actual production publication consumers.
use super::*;
impl FormalBftHeaderNodeDriverV3 {
    #[cfg(test)]
    pub(crate) fn retain_publication_integrity_fixture_for_test(
        &mut self,
        expected: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
        reject_budget: bool,
    ) -> Result<String, String> {
        self.validate_candidate(expected, candidate)?;
        let executed = self.execution.origin.execute(&candidate.actions_cbor)?;
        let nodes = self.execution.origin.retained_nodes.accessed_nodes()?;
        let prefix = self
            .execution
            .ledger
            .records()
            .map(|record| record.input().certificate)
            .collect::<Vec<_>>();
        let plan = super::super::super::publication_plan::PublicationPlanV1::from_actual(
            &self.execution.origin.schedule,
            &executed,
            &candidate.actions_cbor,
            &nodes,
            &prefix,
            storage::RECORD_DOMAIN,
        )?;
        self.private
            .retain_planned(candidate, &plan, &nodes, &mut |bytes, future| {
                if reject_budget {
                    return Err("injected publication reserve rejection".into());
                }
                self.execution.ledger.reserve_private_material(
                    bytes,
                    future.clone(),
                    &self.execution.store,
                )
            })?;
        self.private
            .publication_reference(&candidate.public_value)?
            .ok_or("fixture publication reference missing".into())
    }

    #[cfg(test)]
    pub(crate) fn publication_bounds_for_test(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(usize, usize, usize, FormalBftCertifiedHeaderV3), String> {
        self.validate_candidate(context, candidate)?;
        let executed = self.execution.origin.execute(&candidate.actions_cbor)?;
        let nodes = self.execution.origin.retained_nodes.accessed_nodes()?;
        let prefix = self
            .execution
            .ledger
            .records()
            .map(|record| record.input().certificate)
            .collect::<Vec<_>>();
        let plan = super::super::super::publication_plan::PublicationPlanV1::from_actual(
            &self.execution.origin.schedule,
            &executed,
            &candidate.actions_cbor,
            &nodes,
            &prefix,
            storage::RECORD_DOMAIN,
        )?;
        Ok((
            plan.record_upper_bytes,
            plan.prefix_upper_bytes,
            plan.publication_bytes,
            plan.encoded_qc,
        ))
    }

    #[cfg(test)]
    pub(crate) fn publication_prefix_codec_bound_for_test(
        prefix: &[FormalBftCertifiedHeaderV3],
        next: &FormalBftCertifiedHeaderV3,
    ) -> Result<usize, String> {
        super::super::super::publication_plan::encoded_prefix_upper_bytes(prefix, next)
    }

    #[cfg(test)]
    pub(crate) fn publication_usage_for_test(
        &self,
    ) -> Result<(usize, usize, usize, usize), String> {
        let (total, external, physical) = self.execution.ledger.publication_usage_for_test();
        let consumed = self
            .execution
            .ledger
            .publication_accounting(&self.execution.store)?
            .total_consumed()?;
        Ok((total, external, physical, consumed))
    }

    #[cfg(test)]
    pub(crate) fn reject_live_capacity_parent_for_test(&self) -> Result<(), String> {
        self.execution
            .origin
            .capacity_in_new_layer(&[0x80])
            .map(|_| ())
    }

    pub(crate) fn replace_record_with_foreign_publication_for_test(
        &self,
        height: u64,
    ) -> Result<(), String> {
        let original = self
            .execution
            .ledger
            .record_at(height)
            .ok_or("test Record missing")?;
        let reference = original
            .publication_reservation_ref()
            .ok_or("test original promise missing")?;
        let bytes = common_storage::read_cas_bounded(
            &self.execution.store,
            reference,
            super::super::super::publication_reservation::MAX_PUBLICATION_METADATA,
        )?;
        let mut fields: serde_cbor::Value =
            serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
        let serde_cbor::Value::Map(ref mut fields) = fields else {
            return Err("test metadata map missing".into());
        };
        fields.insert(
            serde_cbor::Value::Text("instance_id".into()),
            serde_cbor::Value::Text("ff".repeat(32)),
        );
        let metadata: super::super::super::publication_reservation::PublicationReservationV1 =
            serde_cbor::value::from_value(serde_cbor::Value::Map(fields.clone()))
                .map_err(|e| e.to_string())?;
        let bytes = metadata.bytes()?;
        let reference = blake3_hex(&bytes);
        common_storage::put_cas_durable(
            &self.execution.store,
            &bytes,
            super::super::super::publication_reservation::MAX_PUBLICATION_METADATA,
        )?;
        let foreign = original
            .clone()
            .with_publication_reservation(Some(reference))?;
        let bytes = serde_cbor::to_vec(&foreign).map_err(|e| e.to_string())?;
        std::fs::write(
            self.records_root
                .join(storage::DIRECTORY)
                .join(format!("record-{height:020}.cbor")),
            bytes,
        )
        .map_err(|e| e.to_string())?;
        let shard = format!(
            "origin-{}-height-{height:020}",
            self.execution.ledger.origin_digest()?
        );
        self.execution
            .store
            .replace_pin_scope_shard(storage::DIRECTORY, &shard, &foreign.references())
            .map_err(|e| format!("test foreign pin {e:?}"))?;
        Ok(())
    }
}

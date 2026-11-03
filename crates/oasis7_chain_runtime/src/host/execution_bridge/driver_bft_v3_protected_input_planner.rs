//! Shared deterministic Source/Active protected publication planning.
use super::*;

impl NodePrivateStore {
    pub(in super::super::super) fn plan_source_admission(
        &self,
        candidate: &NodeBftPrivateCandidateV3,
        plan: &super::super::super::super::super::publication_plan::PublicationPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        kind: SourceSlotKindV1,
    ) -> Result<SourceAdmissionPlanV1, String> {
        self.plan_protected_admission(candidate, plan, nodes, kind, false)
    }

    pub(in super::super::super) fn plan_active_admission(
        &self,
        candidate: &NodeBftPrivateCandidateV3,
        plan: &super::super::super::super::super::publication_plan::PublicationPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        kind: SourceSlotKindV1,
        retained: &BTreeSet<String>,
    ) -> Result<SourceAdmissionPlanV1, String> {
        let mut planned = self.plan_protected_admission(candidate, plan, nodes, kind, true)?;
        planned.reservation_bytes = active_admission::active_reservation_cost(
            &planned.data,
            &planned.publication,
            nodes,
            retained,
        )?;
        Ok(planned)
    }

    fn plan_protected_admission(
        &self,
        candidate: &NodeBftPrivateCandidateV3,
        plan: &super::super::super::super::super::publication_plan::PublicationPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        kind: SourceSlotKindV1,
        active: bool,
    ) -> Result<SourceAdmissionPlanV1, String> {
        let origin = &self
            .origin
            .as_ref()
            .ok_or("source original private Origin absent")?
            .data;
        if origin.schema_version != if active { 3 } else { 8 } {
            return Err("protected input producer requires its exact original owner domain".into());
        }
        let height = candidate.public_value.execution_commitment.height;
        self.check_counts(Some((height, candidate.public_value.block_id()?)))?;
        if height <= origin.height || height - origin.height > 64 {
            return Err("source private candidate height capacity".into());
        }
        let unbound = Data {
            schema_version: 4,
            domain: MULTI_DOMAIN.into(),
            origin: false,
            ledger_origin_digest: origin.ledger_origin_digest.clone(),
            material_identity: origin.material_identity.clone(),
            instance_id: self.instance_id.clone(),
            height,
            value: Some(candidate.public_value.clone()),
            actions_cbor: candidate.actions_cbor.clone(),
            references: nodes.keys().cloned().collect(),
            publication_reservation_ref: None,
            publication_original_data_ref: None,
            source_activation: None,
            protected_source_origin_ref: None,
        };
        let publication =
            PublicationReservationV1::from_plan(&publication_identity(&unbound)?, plan)?;
        let mut data = unbound.clone();
        data.schema_version = if kind == SourceSlotKindV1::Local {
            if active { 11 } else { 9 }
        } else {
            if active { 12 } else { 10 }
        };
        data.domain = if active {
            if kind == SourceSlotKindV1::Local {
                ACTIVE_LOCAL_INPUT_DOMAIN
            } else {
                ACTIVE_RECEIVED_INPUT_DOMAIN
            }
        } else if kind == SourceSlotKindV1::Local {
            PROTECTED_INPUT_DOMAIN
        } else {
            PROTECTED_RECEIVED_INPUT_DOMAIN
        }
        .into();
        data.protected_source_origin_ref = Some(origin.digest()?);
        data.publication_reservation_ref = Some(publication.digest()?);
        publication.validate_identity(&publication_identity(&data)?)?;
        if (kind == SourceSlotKindV1::Local
            && (self.candidates.contains_key(&height) || self.selections.contains_key(&height)))
            || self.exact_slot(&candidate.public_value)?.is_some()
        {
            return Err(
                "source local original already retained; resume same protected input".into(),
            );
        }
        let original_usage = self.usage()?;
        let growth = data
            .bytes()?
            .len()
            .checked_sub(unbound.bytes()?.len())
            .and_then(|n| n.checked_mul(2))
            .ok_or("source Data identity byte growth overflow")?;
        let full_usage = self
            .usage_with(Some(&unbound), nodes)?
            .checked_add(publication.accounted_bytes()?)
            .and_then(|n| n.checked_add(growth))
            .and_then(|n| n.checked_add(private_pin_bound(&data).ok()?))
            .and_then(|n| n.checked_add(source_selection_bound(&data).ok()?))
            .filter(|n| *n <= 512 * 1024 * 1024)
            .ok_or("protected source publication capacity overflow")?;
        let mut future = self.publication_promises()?;
        future.insert(
            publication.digest()?,
            publication.promise_for_identity(&publication_identity(&data)?)?,
        );
        let reservation_bytes = u64::try_from(
            full_usage
                .checked_sub(original_usage)
                .ok_or("protected source publication cannot refund original usage")?,
        )
        .map_err(|_| "source publication byte conversion")?;
        if reservation_bytes == 0 {
            return Err(
                "source original candidate already exists; resume original input instead".into(),
            );
        }
        Ok(SourceAdmissionPlanV1 {
            data,
            publication,
            full_usage,
            future,
            original_usage,
            reservation_bytes,
            resumed: false,
        })
    }
}

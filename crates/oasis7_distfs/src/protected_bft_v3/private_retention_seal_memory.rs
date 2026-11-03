//! Software-memory CAS ownership envelope; not durable persistence evidence.
use super::*;
/// Transient complete ownership image for the dedicated atomic transaction.
/// Persist current State/map, the new immutable seal and its compact original
/// operation together with one canonical historical input/seal index. Do not
/// copy the full historical map into every saved operation: that creates an
/// unaccounted quadratic metadata inventory. Past operation readback reconstructs
/// its exact immutable membership from the canonical original index.
/// This image never enters the signer wire, which carries only bounded refs.
#[derive(Clone, PartialEq, Eq)]
pub struct BftPrivateRetentionSealCasV1 {
    pub before: crate::protected_bft_v3::private_retention_backend::BftPrivateRetentionCurrentV1,
    pub previous_seals:
        std::collections::BTreeMap<String, std::sync::Arc<BftPrivateRetentionSealV1>>,
    pub previous_inputs:
        crate::protected_bft_v3::private_retention_backend::BftPrivateRetentionInputInventoryV1,
    pub seal: std::sync::Arc<BftPrivateRetentionSealV1>,
    pub after: crate::protected_bft_v3::private_retention_backend::BftPrivateRetentionSnapshotV1,
    pub retained_seals:
        std::collections::BTreeMap<String, std::sync::Arc<BftPrivateRetentionSealV1>>,
    pub retained_inputs:
        crate::protected_bft_v3::private_retention_backend::BftPrivateRetentionInputInventoryV1,
}

impl BftPrivateRetentionSealCasV1 {
    /// Backend-neutral original bytes/State envelope. The backend must also
    /// compare this before image to its actual current lease and immutable
    /// original history in the same atomic operation. No pointer identity is
    /// required by this durable-backend contract.
    pub fn validate_envelope(
        &self,
        trusted: &BftPrivateRetentionAttestorV1,
    ) -> Result<(), BftSignerErrorV3> {
        let old = self
            .before
            .retention
            .as_ref()
            .ok_or_else(|| invalid("retention seal missing original ledger"))?;
        self.seal
            .body
            .validate_applied_cut(trusted, &self.before.signer.state)?;
        if old != &self.seal.body.original {
            return Err(invalid("retention seal replaced original map"));
        }
        let mut ordered = Vec::new();
        let mut predecessor = old.map.previous_seal_receipt_digest.clone();
        let mut visited = std::collections::BTreeSet::new();
        while let Some(id) = predecessor {
            if !visited.insert(id.clone()) {
                return Err(invalid("retention seal previous chain cycle"));
            }
            let seal = self
                .previous_seals
                .get(&id)
                .ok_or_else(|| invalid("retention seal previous chain gap"))?;
            if seal.body.digest()? != id {
                return Err(invalid("retention seal previous index differs"));
            }
            predecessor = seal.body.original.map.previous_seal_receipt_digest.clone();
            ordered.push(seal.clone());
        }
        if visited.len() != self.previous_seals.len() {
            return Err(invalid("retention seal previous orphan segment"));
        }
        ordered.reverse();
        ValidatedBftPrivateRetentionSealChainV1::verify(trusted, ordered, old)?;
        verify_signature(
            SEAL_DOMAIN,
            &self.seal.anchor()?.body,
            &self.seal.signature_hex,
            &trusted.provider_public_key,
        )?;
        crate::protected_bft_v3::private_retention_current_anchor::verify_current_anchor(
            trusted,
            &self.after.retention,
            Some(&self.seal.anchor()?),
        )?;
        let before = &self.before.signer;
        let after = &self.after.signer;
        let receipt = &self.after.retention.receipt;
        let previous_digest = digest_hex("oasis7.bft.private-retention.v1.receipt", &old.receipt)?;
        if before.lease_generation != after.lease_generation
            || before.lease_token_digest != after.lease_token_digest
            || before.lease_token_digest.is_empty()
            || before.protected_time_floor_ms != after.protected_time_floor_ms
            || before.successor_archive_transfer != after.successor_archive_transfer
            || before.state.private_retention.as_ref() != Some(&old.receipt.body.head)
            || before.state.binding_digest != trusted.source.binding_digest
            || receipt.body.original_lease_generation != before.lease_generation
            || receipt.body.before_revision != before.state.revision
            || receipt.body.before_state_digest != before.state.state_digest
            || receipt.body.after_revision != after.state.revision
            || receipt.body.after_state_digest != after.state.state_digest
            || receipt.body.head.previous_receipt_digest.as_ref() != Some(&previous_digest)
            || receipt.body.head.sequence
                != old
                    .receipt
                    .body
                    .head
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| invalid("retention seal sequence overflow"))?
            || before
                .state
                .with_private_retention_head(receipt.body.head.clone())?
                != after.state
            || self.after.retention.map != self.seal.body.carry_map()?
        {
            return Err(invalid(
                "retention seal atomic State/receipt envelope differs",
            ));
        }
        crate::protected_bft_v3::private_retention_backend::validate_input_inventory(
            old,
            &self.before.inputs,
        )?;
        crate::protected_bft_v3::private_retention_backend::validate_input_inventory(
            &self.after.retention,
            &self.after.inputs,
        )?;
        let mut inputs = self.previous_inputs.clone();
        for (id, input) in &self.before.inputs {
            if inputs.get(id).is_some_and(|original| original != input) {
                return Err(invalid("retention seal historical original changed"));
            }
            inputs.insert(id.clone(), input.clone());
        }
        let mut seals = self.previous_seals.clone();
        if seals
            .insert(self.seal.body.digest()?, self.seal.clone())
            .is_some()
            || seals != self.retained_seals
            || inputs != self.retained_inputs
            || self
                .after
                .inputs
                .iter()
                .any(|(id, input)| inputs.get(id) != Some(input))
        {
            return Err(invalid("retention seal immutable inventory changed"));
        }
        let expected_material = {
            if self.after.retention.selection_index != self.seal.body.carry_selection_index()? {
                return Err(invalid("retention seal omitted original choices"));
            }
            crate::protected_bft_v3::private_retention_backend::retention_material_bound(
                before,
                Some(old),
                &self.after.retention.map,
                &receipt.body.head,
                &self.after.inputs,
            )?
            .checked_add(self.seal.canonical_bytes()?.len() as u64)
            .ok_or_else(|| invalid("retention seal material bound overflow"))?
        };
        if !matches!(receipt.body.head.schema_version, 2 | 3)
            || receipt.body.head.retained_material_bytes != Some(expected_material)
        {
            return Err(invalid("retention seal exact material reservation differs"));
        }
        crate::protected_bft_v3::archive_transfer::validate_successor_archive_budget(
            &after.state,
            after.successor_archive_transfer.as_ref(),
        )
    }

    pub fn validate_exact_inventory(&self) -> Result<(), BftSignerErrorV3> {
        let old = self
            .before
            .retention
            .as_ref()
            .ok_or_else(|| invalid("retention seal missing current original ledger"))?;
        crate::protected_bft_v3::private_retention_backend::validate_input_inventory(
            old,
            &self.before.inputs,
        )?;
        crate::protected_bft_v3::private_retention_backend::validate_input_inventory(
            &self.after.retention,
            &self.after.inputs,
        )?;
        if old != &self.seal.body.original
            || self.after.retention.map != self.seal.body.carry_map()?
            || self.retained_seals.len()
                != self
                    .previous_seals
                    .len()
                    .checked_add(1)
                    .ok_or_else(|| invalid("retention seal count overflow"))?
        {
            return Err(invalid(
                "retention seal atomic original/carry inventory differs",
            ));
        }
        let digest = self.seal.body.digest()?;
        if self.previous_seals.contains_key(&digest)
            || self
                .retained_seals
                .get(&digest)
                .is_none_or(|seal| !std::sync::Arc::ptr_eq(seal, &self.seal))
            || self.previous_seals.iter().any(|(id, original)| {
                self.retained_seals
                    .get(id)
                    .is_none_or(|retained| !std::sync::Arc::ptr_eq(original, retained))
            })
        {
            return Err(invalid(
                "retention seal replaced or recopied immutable history",
            ));
        }
        let mut expected_inputs = self.previous_inputs.clone();
        for (digest, input) in &self.before.inputs {
            if let Some(original) = expected_inputs.get(digest) {
                if !std::sync::Arc::ptr_eq(original, input) {
                    return Err(invalid("retention seal changed historical input ownership"));
                }
            } else {
                expected_inputs.insert(digest.clone(), input.clone());
            }
        }
        if expected_inputs.len() != self.retained_inputs.len()
            || expected_inputs.iter().any(|(id, original)| {
                self.retained_inputs
                    .get(id)
                    .is_none_or(|retained| !std::sync::Arc::ptr_eq(original, retained))
            })
            || self.after.inputs.iter().any(|(id, carried)| {
                self.retained_inputs
                    .get(id)
                    .is_none_or(|original| !std::sync::Arc::ptr_eq(original, carried))
            })
        {
            return Err(invalid(
                "retention seal lost, added or recopied original input",
            ));
        }
        Ok(())
    }

    /// Software-memory CAS ownership check only. Arc addresses are never a
    /// persistence/authenticity criterion for a durable backend or cold restore.
    /// A future durable implementation must stream exact canonical originals
    /// under its existing real read/decode peak permits instead of using this
    /// in-process helper as a substitute for physical persistence evidence.
    pub fn validate_in_process_readback(&self, actual: &Self) -> Result<(), BftSignerErrorV3> {
        if actual != self
            || !std::sync::Arc::ptr_eq(&self.seal, &actual.seal)
            || !same_seal_ownership(&self.previous_seals, &actual.previous_seals)
            || !same_seal_ownership(&self.retained_seals, &actual.retained_seals)
            || !same_input_ownership(&self.before.inputs, &actual.before.inputs)
            || !same_input_ownership(&self.previous_inputs, &actual.previous_inputs)
            || !same_input_ownership(&self.after.inputs, &actual.after.inputs)
            || !same_input_ownership(&self.retained_inputs, &actual.retained_inputs)
        {
            return Err(invalid(
                "retention in-process seal CAS readback ownership changed",
            ));
        }
        actual.validate_exact_inventory()
    }
}

fn same_seal_ownership(
    expected: &std::collections::BTreeMap<String, std::sync::Arc<BftPrivateRetentionSealV1>>,
    actual: &std::collections::BTreeMap<String, std::sync::Arc<BftPrivateRetentionSealV1>>,
) -> bool {
    expected.len() == actual.len()
        && expected.iter().all(|(key, value)| {
            actual
                .get(key)
                .is_some_and(|other| std::sync::Arc::ptr_eq(value, other))
        })
}
fn same_input_ownership(
    expected: &crate::protected_bft_v3::private_retention_backend::BftPrivateRetentionInputInventoryV1,
    actual: &crate::protected_bft_v3::private_retention_backend::BftPrivateRetentionInputInventoryV1,
) -> bool {
    expected.len() == actual.len()
        && expected.iter().all(|(key, value)| {
            actual
                .get(key)
                .is_some_and(|other| std::sync::Arc::ptr_eq(value, other))
        })
}

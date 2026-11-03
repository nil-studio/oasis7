//! Dedicated seal transaction. Ordinary Reserve/Ready never changes a floor.
use super::*;
use crate::protected_bft_v3::private_retention_seal::*;
use std::sync::Arc;

impl BftSignerServiceV3 {
    pub(super) fn private_retention_seal_command(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        snapshot: BftSignerBackendSnapshotV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let BftSignerCommandV3::SealPrivateRetention {
            expected_revision,
            expected_state_digest,
            cut,
        } = &request.body.command
        else {
            return Err(invalid("retention seal command mismatch"));
        };
        let support = self
            .private_retention_support
            .as_ref()
            .ok_or_else(|| invalid("retention seal independent capability unavailable"))?;
        let deadline = runtime
            .successor_deadline
            .ok_or_else(|| invalid("retention seal absolute deadline unavailable"))?;
        live(deadline)?;
        let observed = runtime.backend.read_private_retention_sealed_current(
            snapshot.lease_generation,
            &snapshot.lease_token_digest,
        )?;
        if observed.current.signer != snapshot {
            return Err(invalid(
                "retention seal original backend observation differs",
            ));
        }
        observed.validate_inventory(&self.binding, &support.attestor)?;
        let old = observed
            .current
            .retention
            .as_ref()
            .ok_or_else(|| invalid("uncovered source cannot seal inferred coverage"))?;
        // Acquiring a new fenced lease does not turn the historical receipt
        // into current authority. Explicitly read its original operation before
        // allowing another mutation in this namespace.
        let original_operation = runtime.backend.read_private_retention_operation(
            snapshot.lease_generation,
            &snapshot.lease_token_digest,
            &old.receipt.body.operation_digest,
        )?;
        let receipt_id = digest_hex("oasis7.bft.private-retention.v1.receipt", &old.receipt)?;
        if observed
            .history
            .get(&receipt_id)
            .is_none_or(|expected| expected.as_ref() != &original_operation)
            || original_operation.retention != *old
            || original_operation.inputs != observed.current.inputs
        {
            runtime.poisoned = true;
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid(
                "retention seal original operation observation differs",
            ));
        }
        // An unknown outcome/new-lease retry retrieves the same original op.
        // No second seal or receipt is signed from the caller's request.
        if let Some(seal) = observed.latest_seal()? {
            if seal.body.cut == **cut
                && old.receipt.body.before_revision == *expected_revision
                && old.receipt.body.before_state_digest == *expected_state_digest
            {
                let original = &original_operation;
                if original.retention != *old
                    || original.signer.state != snapshot.state
                    || original.retention.map != seal.body.carry_map()?
                {
                    runtime.poisoned = true;
                    runtime.input_view_epoch.invalidate()?;
                    return Err(invalid("retention original seal retry readback differs"));
                }
                return self.private_retention_reply_with_anchor(
                    runtime,
                    snapshot,
                    old.clone(),
                    Some(seal.anchor()?),
                    request,
                    request_digest,
                );
            }
        }
        if *expected_revision != snapshot.state.revision
            || *expected_state_digest != snapshot.state.state_digest
        {
            return Err(invalid("retention seal expected State differs"));
        }
        let body = BftPrivateRetentionSealBodyV1::from_original(old.clone(), (**cut).clone())?;
        body.validate_applied_cut(&support.attestor, &snapshot.state)?;
        let context = crate::protected_bft_v3::private_retention_seal_context::BftPrivateRetentionSealVerificationContextV1::from_backend_sealed_current_with_verified_state(
            &self.binding, &support.attestor, &observed, &snapshot, runtime.verified.as_ref(),
        )?;
        support
            .verifier
            .verify_seal_with_context(&self.binding, cut, &context, deadline)?;
        live(deadline)?;
        let seal = Arc::new(BftPrivateRetentionSealV1 {
            signature_hex: sign_preimage(self.provider_key.as_ref(),
                crate::protected_bft_v3::private_retention_current_anchor::SEAL_ANCHOR_DOMAIN,
                &crate::protected_bft_v3::private_retention_current_anchor::BftPrivateRetentionSealAnchorBodyV1::from_original(&body)?)?,
            body,
        });
        let map = seal.body.carry_map()?;
        let selection_index = seal.body.carry_selection_index()?;
        let inputs = observed
            .current
            .inputs
            .iter()
            .filter(|(_, input)| input.key.height > cut.height)
            .map(|(id, input)| (id.clone(), input.clone()))
            .collect();
        let bytes = map.canonical_bytes()?;
        let mut head = BftPrivateRetentionHeadV1 {
            selection_commitment: selection_index
                .as_ref()
                .map(|i| i.commitment())
                .transpose()?,
            schema_version: if selection_index.is_some() { 3 } else { 2 },
            source: map.source.clone(),
            capability_digest: support.attestor.digest()?,
            sequence: old
                .receipt
                .body
                .head
                .sequence
                .checked_add(1)
                .ok_or_else(|| invalid("retention seal sequence overflow"))?,
            map_digest: map.digest()?,
            map_byte_hash: blake3::hash(&bytes).to_hex().to_string(),
            entry_count: u32::try_from(map.entries.len())
                .map_err(|_| invalid("retention seal count overflow"))?,
            previous_receipt_digest: Some(digest_hex(
                "oasis7.bft.private-retention.v1.receipt",
                &old.receipt,
            )?),
            retained_material_bytes: None,
        };
        head.retained_material_bytes = Some(
            retention_material_bound(&snapshot, Some(old), &map, &head, &inputs)?
                .checked_add(seal.canonical_bytes()?.len() as u64)
                .ok_or_else(|| invalid("retention seal material overflow"))?,
        );
        let next = snapshot.state.with_private_retention_head(head.clone())?;
        let receipt_body = BftPrivateRetentionReceiptBodyV1 {
            head,
            operation_digest: digest_hex(
                "oasis7.bft.private-retention.seal-operation.v1",
                &(
                    &seal.body.digest()?,
                    &map,
                    &old.receipt.body.operation_digest,
                ),
            )?,
            original_lease_generation: snapshot.lease_generation,
            before_revision: snapshot.state.revision,
            before_state_digest: snapshot.state.state_digest.clone(),
            after_revision: next.revision,
            after_state_digest: next.state_digest.clone(),
        };
        let receipt = BftPrivateRetentionReceiptV1 {
            signature_hex: sign_preimage(
                self.provider_key.as_ref(),
                RECEIPT_DOMAIN,
                &receipt_body,
            )?,
            body: receipt_body,
        };
        let mut after = snapshot.clone();
        after.state = next;
        let mut retained_seals = observed.seals.clone();
        retained_seals.insert(seal.body.digest()?, seal.clone());
        let mut retained_inputs = observed.sealed_inputs.clone();
        for (id, input) in &observed.current.inputs {
            if retained_inputs
                .get(id)
                .is_some_and(|old| old.as_ref() != input.as_ref())
            {
                return Err(invalid("retention seal original input union conflicts"));
            }
            retained_inputs.insert(id.clone(), input.clone());
        }
        let transaction = BftPrivateRetentionSealCasV1 {
            before: observed.current.clone(),
            previous_seals: observed.seals.clone(),
            previous_inputs: observed.sealed_inputs.clone(),
            seal: seal.clone(),
            after: BftPrivateRetentionSnapshotV1 {
                signer: after,
                retention: BftPrivateRetentionLedgerV1 {
                    selection_index,
                    map,
                    receipt,
                },
                inputs,
            },
            retained_seals,
            retained_inputs,
        };
        transaction.validate_envelope(&support.attestor)?;
        // Check the complete bounded reply against the same namespace budget
        // before signing becomes externally visible. This does not charge yet.
        self.private_retention_reply_preflight_with_anchor(
            &transaction.after.signer,
            &transaction.after.retention,
            Some(&seal.anchor()?),
            request,
            request_digest,
        )?;
        live(deadline)?;
        runtime.input_view_epoch.invalidate()?;
        let committed = match runtime
            .backend
            .compare_and_swap_private_retention_seal(&transaction)
        {
            Ok(value) => value,
            Err(error) => {
                runtime.poisoned = true;
                runtime.input_view_epoch.invalidate()?;
                return Err(invalid(format!(
                    "retention seal CAS outcome unknown; provider poisoned: {error}"
                )));
            }
        };
        if committed != transaction.after {
            runtime.poisoned = true;
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid(
                "retention seal original operation readback differs",
            ));
        }
        let readback = match runtime.backend.read_private_retention_sealed_current(
            committed.signer.lease_generation,
            &committed.signer.lease_token_digest,
        ) {
            Ok(value) => value,
            Err(error) => {
                runtime.poisoned = true;
                runtime.input_view_epoch.invalidate()?;
                return Err(invalid(format!(
                    "retention seal original history readback unavailable: {error}"
                )));
            }
        };
        if readback.current.signer != committed.signer
            || readback.current.retention.as_ref() != Some(&committed.retention)
            || readback.current.inputs != committed.inputs
            || readback.seals != transaction.retained_seals
            || readback.sealed_inputs != transaction.retained_inputs
            || readback
                .validate_inventory(&self.binding, &support.attestor)
                .is_err()
        {
            runtime.poisoned = true;
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid("retention seal atomic history readback differs"));
        }
        runtime.verified =
            match self.validate_provider_state(&committed.signer.state, Some(deadline)) {
                Ok(value) => Some(value),
                Err(error) => {
                    runtime.poisoned = true;
                    runtime.input_view_epoch.invalidate()?;
                    return Err(error);
                }
            };
        live(deadline)?;
        self.private_retention_reply_with_anchor(
            runtime,
            committed.signer,
            committed.retention,
            Some(seal.anchor()?),
            request,
            request_digest,
        )
    }
}

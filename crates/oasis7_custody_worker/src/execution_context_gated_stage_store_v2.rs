//! Explicit per-leaf Identity stage mode. It cannot upgrade an old Native10 row.
use super::*;
/// First-only handler ownership. This is never reconstructed from a SQL row.
pub(crate) struct NativePrepareClaim {
    operation: String,
    proof: String,
    request: NativeExecutionPrepareRequestV2,
    previous: std::cell::RefCell<
        oasis7_custody_core::execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2,
    >,
}
pub(crate) fn claim_native_prepare_gated(
    state: &JsValue,
    env: &worker::Env,
    frame: crate::execution_dispatch_rpc_v2::ExecutionDispatchFrameV2,
) -> Result<NativePrepareClaim> {
    let now = js_sys::Date::now();
    if !now.is_finite() || now < 0.0 || now.fract() != 0.0 || now > 9_007_199_254_740_991.0 {
        return Err(rejected());
    }
    frame.check(
        env,
        ExecutionPreparePhysicalEdgeV2::IdentityServiceToNativeActor,
        now as u64,
    )?;
    let q = *frame.request;
    validate_snapshot(state)?;
    validate_dispatch_history(state, env)?;
    q.wire_bytes().map_err(|_| rejected())?;
    let c = &q.proof.certificate;
    let op = operation(
        "prepare_context_gated_v2",
        &c.session_id,
        &c.approved_input_digest,
        &c.stage_operation_id,
    )?;
    let proof = q.proof.digest().map_err(|_| rejected())?;
    let raw = serde_json::to_string(&Claim {
        operation_id: &op,
        profile: "prepare_context_gated_v2",
        proof_digest: &proof,
        scope_digest: hash(&c.session.scope)?,
        payload: serde_json::to_string(&q).map_err(|_| rejected())?,
        actor_tag: actor_tag(&c.session)?,
        issuer_namespace: &c.issuer.namespace,
        issuer_service: &c.issuer.service_id,
        environment: &c.issuer.environment,
        issuer_epoch: c.issuer.issuer_epoch.0.to_string(),
        config_generation: c.issuer.config_generation.0.to_string(),
        first_ports: [&c.ports[0].attempt_id, &c.ports[1].attempt_id],
        dispatch_receipt: Some(
            serde_json::to_string(&frame.authorization).map_err(|_| rejected())?,
        ),
    })
    .map_err(|_| rejected())?;
    if claim(state, &raw)? != op {
        return Err(rejected());
    }
    Ok(NativePrepareClaim {
        operation: op,
        proof,
        request: q,
        previous: std::cell::RefCell::new(frame.authorization),
    })
}
impl NativePrepareClaim {
    pub(crate) fn request(&self) -> &NativeExecutionPrepareRequestV2 {
        &self.request
    }
    pub(crate) async fn claim_before_io(
        &self,
        state: &JsValue,
        env: &worker::Env,
        edge: ExecutionPreparePhysicalEdgeV2,
        body_digest: String,
    ) -> Result<oasis7_custody_core::execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2>
    {
        validate_dispatch_history(state, env)?;
        let index = self
            .request
            .proof
            .certificate
            .ports
            .iter()
            .position(|p| p.edge == edge)
            .ok_or_else(rejected)?;
        if !(2..=6).contains(&index) && index != 9 {
            return Err(rejected());
        }
        let previous = self.previous.borrow().clone();
        let signed = crate::identity_execution_dispatch_client_v2::claim(
            env,
            &self.request,
            &previous,
            edge,
            body_digest,
        )
        .await?;
        // Account's permanent claim linearized against Finish/Abort before
        // this local once. Failure in either place never releases either claim.
        if index != 9 {
            let port = &self.request.proof.certificate.ports[index];
            let raw = serde_json::to_string(&signed).map_err(|_| rejected())?;
            claim_edge(
                state,
                &self.operation,
                &self.proof,
                index as u8,
                &port.attempt_id,
                &raw,
            )?;
            validate_snapshot(state)?;
        }
        *self.previous.borrow_mut() = signed.clone();
        Ok(signed)
    }
}

impl NativePrepareClaim {
    /// Called only after the fixed Native pin and full SDK signed delivery
    /// checks. This advances no HTTP leaf: World7/8 were consumed in Native DB.
    pub(crate) fn native_completed(
        &self,
        state: &JsValue,
        env: &worker::Env,
        receipts: &[oasis7_custody_core::execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2;
             2],
        now: u64,
    ) -> Result<()> {
        let (pin,floor)=crate::execution_context_issuer_pin_v2::pin(env,
            oasis7_custody_core::native_cleanup_v2::AccountMachinePurposeV2::ExecutionDispatchClaimV2)?;
        let raw = js_sys::Array::new();
        let mut previous = self.previous.borrow().claim.claim_ordinal.0;
        for (index, receipt) in receipts.iter().enumerate() {
            receipt
                .verify_issuer(&self.request, &pin, &floor, now)
                .map_err(|_| rejected())?;
            if receipt.claim.leaf != self.request.proof.certificate.ports[7 + index]
                || receipt.claim.claim_ordinal.0 <= previous
            {
                return Err(rejected());
            }
            previous = receipt.claim.claim_ordinal.0;
            raw.push(&JsValue::from_str(
                &serde_json::to_string(receipt).map_err(|_| rejected())?,
            ));
        }
        native_complete(state, &self.operation, &self.proof, raw.as_ref())?;
        *self.previous.borrow_mut() = receipts[1].clone();
        validate_snapshot(state)
    }
    pub(crate) fn persist_sealed_put(
        &self,
        state: &JsValue,
        record: &oasis7_custody_core::context_body_v2::ContextBodyRecordV2,
        reference: &oasis7_custody_core::context_body_v2::AttestedSlotRefV2,
        receipt: &SignedNativeExecutionPrepareReceiptV2,
    ) -> Result<()> {
        let raw = serde_json::to_string(&(record, reference, receipt)).map_err(|_| rejected())?;
        persist_put(
            state,
            &self.operation,
            &self.proof,
            &self.request.proof.certificate.ports[9].attempt_id,
            &raw,
            &serde_json::to_string(&*self.previous.borrow()).map_err(|_| rejected())?,
        )?;
        validate_snapshot(state)
    }
    pub(crate) fn finish_put(
        self,
        state: &JsValue,
        result: &ExecutionPreparedPublicResultV2,
    ) -> Result<()> {
        if result.proof_digest != self.proof {
            return Err(rejected());
        }
        let raw = serde_json::to_string(result).map_err(|_| rejected())?;
        if complete_put(state, &self.operation, &self.proof, &raw)? != raw {
            return Err(rejected());
        }
        validate_snapshot(state)
    }
}

impl NativePrepareClaim {
    /// Existing same-manifest private2 result is a fresh local witness only
    /// while its original signed window remains valid. No extra RPC occurs.
    pub(crate) fn private_current_proof(
        &self,
        state: &JsValue,
    ) -> Result<SignedAccountExecutionPrivateCurrentV2> {
        validate_snapshot(state)?;
        let snapshot: Snapshot = serde_json::from_str(&read(state)?).map_err(|_| rejected())?;
        let c = &self.request.proof.certificate;
        let row = snapshot
            .records
            .iter()
            .find(|r| {
                r.profile == "private_current_v2"
                    && r.proof_digest == c.private_current_proof_digest
            })
            .ok_or_else(rejected)?;
        let row: Row =
            serde_json::from_str(&read_row(state, &row.operation_id)?).map_err(|_| rejected())?;
        let p: SignedAccountExecutionPrivateCurrentV2 =
            serde_json::from_str(&row.payload).map_err(|_| rejected())?;
        let reply: ExecutionPrivateCurrentReplyV2 =
            serde_json::from_str(row.completed.as_deref().ok_or_else(rejected)?)
                .map_err(|_| rejected())?;
        if p.certificate.session_id != c.session_id
            || p.certificate.approved_input_digest != c.approved_input_digest
            || p.certificate.action_request_id != c.action_request_id
            || p.certificate.stage_operation_id != c.stage_operation_id
            || p.certificate.factory_key != c.factory_record_key
            || p.certificate.source_operation_id != c.source_operation_id
            || p.certificate.session != c.session
            || p.certificate.expires_at.0 < c.expires_at.0
            || reply.proof_digest != c.private_current_proof_digest
        {
            return Err(rejected());
        }
        Ok(p)
    }
}

pub(super) fn validate_receipts(
    state: &JsValue,
    q: &NativeExecutionPrepareRequestV2,
    r: &Row,
) -> Result<()> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StoredReceipt {
        edge: usize,
        receipt: String,
    }
    let records: Vec<StoredReceipt> =
        serde_json::from_str(&read_receipts(state, &r.operation_id)?).map_err(|_| rejected())?;
    let expected: Vec<usize> = match r.next_edge {
        2..=7 => (1..usize::from(r.next_edge)).collect(),
        9 => (1..9).collect(),
        10 => (1..10).collect(),
        _ => return Err(rejected()),
    };
    if records.len() != expected.len() {
        return Err(rejected());
    }
    let mut previous = 0;
    for (record, edge) in records.iter().zip(expected) {
        let receipt: oasis7_custody_core::execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2 =
                            serde_json::from_str(&record.receipt).map_err(|_| rejected())?;
        receipt.claim.validate_relative(q).map_err(|_| rejected())?;
        if record.edge != edge
            || receipt.claim.leaf != q.proof.certificate.ports[edge]
            || receipt.claim.claim_ordinal.0 <= previous
            || receipt.signature_hex.len() != 128
            || !receipt
                .signature_hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(rejected());
        }
        previous = receipt.claim.claim_ordinal.0;
    }

    Ok(())
}

/// All historical receipts are checked against the actual configured Account
/// key, at their original issuance time. This reconstructs no outbound token.
fn validate_dispatch_history(state: &JsValue, env: &worker::Env) -> Result<()> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StoredReceipt {
        edge: usize,
        receipt: String,
    }
    let snapshot: Snapshot = serde_json::from_str(&read(state)?).map_err(|_| rejected())?;
    let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
        env,
        oasis7_custody_core::native_cleanup_v2::AccountMachinePurposeV2::ExecutionDispatchClaimV2,
    )?;
    for header in snapshot
        .records
        .iter()
        .filter(|r| r.profile == "prepare_context_gated_v2")
    {
        let row: Row = serde_json::from_str(&read_row(state, &header.operation_id)?)
            .map_err(|_| rejected())?;
        let q: NativeExecutionPrepareRequestV2 =
            serde_json::from_str(&row.payload).map_err(|_| rejected())?;
        let records: Vec<StoredReceipt> =
            serde_json::from_str(&read_receipts(state, &row.operation_id)?)
                .map_err(|_| rejected())?;
        for record in records {
            let signed:oasis7_custody_core::execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2=
                serde_json::from_str(&record.receipt).map_err(|_|rejected())?;
            signed
                .verify_issuer(&q, &pin, &floor, signed.claim.issued_at.0)
                .map_err(|_| rejected())?;
            if q.proof.certificate.ports.get(record.edge) != Some(&signed.claim.leaf) {
                return Err(rejected());
            }
        }
    }
    Ok(())
}

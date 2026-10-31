use super::*;

impl ProviderOriginalMaterialOwnerV1 {
    pub(super) fn verify_commit_materials_impl(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        request: &ContinuousAgentRequestContextV1,
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<(), String> {
        let inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let (_, record) = self
            .latest_event(
                &inner.head,
                &materials.operation_id,
                &inner.head.seen,
                MemoryClass::Ordinary,
                &mut physical,
            )?
            .ok_or("provider committed materials missing from seen index")?;
        if record.phase != EventPhase::ResponseCaptured
            || self.refs_from_event(&record)? != *materials
        {
            return Err(
                "provider committed material references are not the captured attempt".into(),
            );
        }
        let request_context = self.read_object_bytes(
            &record.request_context_object_id,
            MAX_CONTEXT_BYTES,
            MemoryClass::Ordinary,
        )?;
        let stored_request = self
            .memory
            .reserve(MAX_CONTEXT_BYTES, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?
            .retain(
                serde_json::from_slice::<ContinuousAgentRequestContextV1>(
                    request_context.as_slice(),
                )
                .map_err(|error| error.to_string())?,
            );
        if *stored_request != *request
            || (*stored_request).provider_invocation_key() != request.provider_invocation_key()
        {
            return Err("provider request typed context differs from durable original".into());
        }
        let request_bytes = self.json_bytes(request, MAX_CONTEXT_BYTES)?;
        if *blake3::hash(&request_bytes.bytes).as_bytes() != record.request_object_id {
            return Err("provider request context is not the original HTTP bytes".into());
        }
        {
            let original_request_body = self.read_object_bytes(
                &record.request_object_id,
                usize::try_from(MAX_REQUEST_BYTES).map_err(|_| "provider request bound")?,
                MemoryClass::Ordinary,
            )?;
            if original_request_body.as_slice() != request_bytes.bytes.as_slice() {
                return Err("provider original request body differs from its bound context".into());
            }
        }
        let response_context = self.read_object_bytes(
            &record
                .response_context_object_id
                .ok_or("provider response context absent")?,
            MAX_CONTEXT_BYTES,
            MemoryClass::Ordinary,
        )?;
        let stored_response = self
            .memory
            .reserve(MAX_CONTEXT_BYTES, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?
            .retain(
                serde_json::from_slice::<ContinuousAgentResponseContextV1>(
                    response_context.as_slice(),
                )
                .map_err(|error| error.to_string())?,
            );
        if *stored_response != *response {
            return Err("provider response typed context differs from durable original".into());
        }
        let response_body = self.read_object_bytes(
            &record
                .response_object_id
                .ok_or("provider response body absent")?,
            usize::try_from(MAX_RESPONSE_BYTES).map_err(|_| "provider response bound")?,
            MemoryClass::Ordinary,
        )?;
        let decoded = self
            .memory
            .reserve(MAX_CONTEXT_BYTES, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?
            .retain(
                serde_json::from_slice::<ContinuousAgentResponseContextV1>(
                    response_body.as_slice(),
                )
                .map_err(|error| error.to_string())?,
            );
        if *decoded != *response {
            return Err("provider typed response is not the original HTTP body".into());
        }
        Ok(())
    }

    pub(super) fn mark_committed_impl(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String> {
        self.require_writable()?;
        self.authority
            .verify_commit(&self.owner, materials, receipt_id, action_id)?;
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let record = self.latest_for_attempt(
            &inner,
            &self.attempt_from_materials_impl(materials),
            &mut physical,
        )?;
        if record.phase == EventPhase::Committed
            && record.terminal_receipt_id.as_deref() == Some(receipt_id)
            && record.terminal_action_id == Some(action_id)
        {
            return Ok(());
        }
        if record.phase != EventPhase::ResponseCaptured
            || self.refs_from_event(&record)? != *materials
            || receipt_id.is_empty()
            || receipt_id.len() > 256
        {
            return Err("provider commit does not match unresolved captured materials".into());
        }
        let mut terminal = record;
        terminal.phase = EventPhase::Committed;
        terminal.terminal_receipt_id = Some(receipt_id.to_owned());
        terminal.terminal_action_id = Some(action_id);
        if let Err(error) = self.append_event(&mut inner, terminal) {
            inner.poisoned = true;
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn mark_rejected_impl(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        evidence: ProviderDispositionEvidenceV1,
    ) -> Result<(), String> {
        self.require_writable()?;
        self.authority
            .verify_rejection(&self.owner, attempt, evidence)?;
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let mut record = self.latest_for_attempt(&inner, attempt, &mut physical)?;
        if record.phase == EventPhase::Rejected
            && record.terminal_evidence_object_id == Some(evidence.evidence_object_id)
            && record.terminal_disposition_digest == Some(evidence.disposition_digest)
        {
            return Ok(());
        }
        if !record.phase.is_pending() {
            return Err("provider rejection cannot alter a terminal attempt".into());
        }
        // Verify the evidence content is present in this same original store
        // before the pending index is allowed to stop protecting it.
        let _evidence = self.read_object_bytes(
            &evidence.evidence_object_id,
            MAX_CONTEXT_BYTES,
            MemoryClass::Ordinary,
        )?;
        record.phase = EventPhase::RejectionEvidencePlanned;
        record.terminal_evidence_object_id = Some(evidence.evidence_object_id);
        record.terminal_disposition_digest = Some(evidence.disposition_digest);
        if let Err(error) = self.append_event(&mut inner, record.clone()) {
            inner.poisoned = true;
            return Err(error);
        }
        record.phase = EventPhase::Rejected;
        if let Err(error) = self.append_event(&mut inner, record) {
            inner.poisoned = true;
            return Err(error);
        }
        Ok(())
    }
}

impl ProviderOriginalMaterialOwnerV1 {
    pub(super) fn attempt_from_materials_impl(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
    ) -> ProviderMaterialAttemptV1 {
        ProviderMaterialAttemptV1 {
            owner: materials.owner.clone(),
            operation_id: materials.operation_id.clone(),
            transport_attempt: materials.transport_attempt,
            attempt_id: materials.attempt_id,
            parent_root_set: materials.parent_root_set,
            authority_snapshot: materials.authority_snapshot,
            request_object_id: materials.request_object_id,
            request_context_object_id: materials.request_context_object_id,
        }
    }
}

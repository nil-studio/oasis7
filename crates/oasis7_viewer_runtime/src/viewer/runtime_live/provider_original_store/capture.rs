use super::*;

impl ProviderOriginalMaterialOwnerV1 {
    pub(super) fn persist_request_impl(
        &self,
        request: &ContinuousAgentRequestContextV1,
        expected_body_id: [u8; 32],
        mut original_body: &mut dyn Read,
        length: u64,
    ) -> Result<ProviderMaterialAttemptV1, String> {
        self.require_writable()?;
        if length == 0 || length > MAX_REQUEST_BYTES {
            return Err("provider request body bound".into());
        }
        let binding = self.authority.request_binding(&self.owner, request)?;
        let operation_id = request.provider_invocation_key().to_string();
        operation_key(&operation_id)?;
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        if self
            .read_state_value(
                &inner.head.seen,
                &operation_key(&operation_id)?,
                MemoryClass::Ordinary,
                &mut physical,
            )?
            .is_some()
        {
            return Err("provider operation identity was already consumed or dispatched".into());
        }
        let context = self.json_bytes(request, MAX_CONTEXT_BYTES)?;
        let context_id = *blake3::hash(&context.bytes).as_bytes();
        let attempt_id = make_attempt_id(
            &self.owner,
            &operation_id,
            request.transport_attempt,
            binding,
            expected_body_id,
            context_id,
        );
        let mut logical = finite_value_io();
        let objects = IncrementalObjectStore::open(
            &self.pages,
            MemoryClass::Ordinary,
            &mut physical,
            &self.cancelled,
        )
        .map_err(|error| error.to_string())?;
        let actual_body_id = objects
            .write_object(
                &mut original_body,
                length,
                Some(expected_body_id),
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical,
                    class: MemoryClass::Ordinary,
                    cancelled: &self.cancelled,
                },
            )
            .map_err(|error| error.to_string())?;
        if actual_body_id != expected_body_id {
            return Err("provider request original body ID mismatch".into());
        }
        self.persist_object(&context.bytes, context_id)?;
        let record = EventRecord {
            phase: EventPhase::RequestPrepared,
            sequence: 0,
            previous_event_id: [0; 32],
            operation_id: operation_id.clone(),
            transport_attempt: request.transport_attempt,
            attempt_id,
            parent_root_set: binding.parent_root_set,
            authority_snapshot: binding.authority_snapshot,
            request_object_id: actual_body_id,
            request_context_object_id: context_id,
            response_object_id: None,
            response_context_object_id: None,
            response_status: None,
            terminal_receipt_id: None,
            terminal_action_id: None,
            terminal_evidence_object_id: None,
            terminal_disposition_digest: None,
        };
        if let Err(error) = self.append_event(&mut inner, record) {
            inner.poisoned = true;
            return Err(error);
        }
        Ok(ProviderMaterialAttemptV1 {
            owner: self.owner.clone(),
            operation_id,
            transport_attempt: request.transport_attempt,
            attempt_id,
            parent_root_set: binding.parent_root_set,
            authority_snapshot: binding.authority_snapshot,
            request_object_id: actual_body_id,
            request_context_object_id: context_id,
        })
    }

    pub(super) fn mark_dispatched_impl(
        &self,
        attempt: &ProviderMaterialAttemptV1,
    ) -> Result<(), String> {
        self.require_writable()?;
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let record = self.latest_for_attempt(&inner, attempt, &mut physical)?;
        if record.phase != EventPhase::RequestPrepared {
            return Err("provider attempt is not prepared for its first dispatch".into());
        }
        let mut next = record;
        next.phase = EventPhase::DispatchedOrUnknown;
        if let Err(error) = self.append_event(&mut inner, next) {
            inner.poisoned = true;
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn persist_response_body_impl(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        expected_body_id: [u8; 32],
        mut original_body: &mut dyn Read,
        length: u64,
    ) -> Result<[u8; 32], String> {
        self.require_writable()?;
        if length > MAX_RESPONSE_BYTES {
            return Err("provider response body bound".into());
        }
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let mut record = self.latest_for_attempt(&inner, attempt, &mut physical)?;
        if record.phase != EventPhase::DispatchedOrUnknown {
            return Err("provider response has no durable dispatch marker".into());
        }
        record.phase = EventPhase::ResponseBodyPlanned;
        record.response_object_id = Some(expected_body_id);
        record.response_status = Some(status);
        if let Err(error) = self.append_event(&mut inner, record.clone()) {
            inner.poisoned = true;
            return Err(error);
        }
        let mut logical = finite_value_io();
        let objects = IncrementalObjectStore::open(
            &self.pages,
            MemoryClass::Ordinary,
            &mut physical,
            &self.cancelled,
        )
        .map_err(|error| {
            inner.poisoned = true;
            error.to_string()
        })?;
        let actual = objects
            .write_object(
                &mut original_body,
                length,
                Some(expected_body_id),
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical,
                    class: MemoryClass::Ordinary,
                    cancelled: &self.cancelled,
                },
            )
            .map_err(|error| {
                inner.poisoned = true;
                error.to_string()
            })?;
        if actual != expected_body_id {
            inner.poisoned = true;
            return Err("provider response original body ID mismatch".into());
        }
        record.phase = EventPhase::ResponseBodyDurable;
        if let Err(error) = self.append_event(&mut inner, record) {
            inner.poisoned = true;
            return Err(error);
        }
        Ok(actual)
    }

    pub(super) fn finish_response_impl(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        response_object_id: [u8; 32],
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<ProviderOriginalMaterialRefsV1, String> {
        self.require_writable()?;
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let mut record = self.latest_for_attempt(&inner, attempt, &mut physical)?;
        if record.phase != EventPhase::ResponseBodyDurable
            || record.response_object_id != Some(response_object_id)
            || record.response_status != Some(status)
        {
            return Err("provider typed response does not match durable original body".into());
        }
        let context = self.json_bytes(response, MAX_CONTEXT_BYTES)?;
        let context_id = *blake3::hash(&context.bytes).as_bytes();
        record.phase = EventPhase::ResponseContextPlanned;
        record.response_context_object_id = Some(context_id);
        if let Err(error) = self.append_event(&mut inner, record.clone()) {
            inner.poisoned = true;
            return Err(error);
        }
        if let Err(error) = self.persist_object(&context.bytes, context_id) {
            inner.poisoned = true;
            return Err(error);
        }
        record.phase = EventPhase::ResponseCaptured;
        if let Err(error) = self.append_event(&mut inner, record.clone()) {
            inner.poisoned = true;
            return Err(error);
        }
        self.refs_from_event(&record)
    }

    pub(super) fn validate_request_owner_impl(
        &self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<(), String> {
        self.authority.verify_owner_scope(&self.owner)?;
        if request.runtime_binding.world_id != self.owner.world_id {
            return Err("provider request world differs from pending owner".into());
        }
        self.authority.request_binding(&self.owner, request)?;
        Ok(())
    }
}

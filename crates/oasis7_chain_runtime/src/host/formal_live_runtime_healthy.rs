//! Existing healthy durable-set owner methods, with explicit signer capability checks.
use super::*;

impl FormalLiveRuntime {
    /// Arm the exact next healthy durable-set boundary for the currently
    /// running owner. This only persists old-writer-signed intent; it does not
    /// execute a boundary, seal endpoints, or activate a successor authority.
    pub(crate) fn arm_durable_set_transition(
        &self,
        operation_id: &str,
        transition: SignedNormalDurableSetTransitionV1,
    ) -> Result<crate::execution_bridge::formal_durable_set_arm::DurableSetArmStatusV1, String>
    {
        if !self.validated.writer_key.supports_healthy_durable_set() {
            return Err(
                "formal writer custody does not authorize healthy durable-set rotation".into(),
            );
        }
        let _admission = self
            .admission_gate
            .lock()
            .map_err(|_| "formal Node admission gate poisoned")?;
        if !self.started || !self.state.ready.load(std::sync::atomic::Ordering::Acquire) {
            return Err("formal runtime is not ready to admit a durable-set arm".into());
        }
        let cut = self.published.current()?;
        self.durable_set_arm
            .arm(operation_id, transition, &cut.anchor)
    }

    /// Select and sign the next boundary under the same gate as public submit.
    pub(crate) fn prepare_durable_set_transition(
        &self,
        operation_id: &str,
        mut replica: oasis7_distfs::controlled_authority::formal::EndpointSignerBindingV1,
        timeout: Duration,
    ) -> Result<SignedNormalDurableSetTransitionV1, String> {
        use oasis7_distfs::controlled_authority::formal::NormalDurableSetTransitionBodyV1;
        if !self.validated.writer_key.supports_healthy_durable_set() {
            return Err(
                "formal writer custody does not authorize healthy durable-set rotation".into(),
            );
        }
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err("durable-set preparation timeout in (0,60s]".into());
        }
        let deadline =
            std::time::Instant::now() + timeout.saturating_sub(Duration::from_millis(100));
        {
            let _admission = quiesce::acquire_for_idempotency_read(&self.admission_gate, deadline)?;
            let cut = self.published.current()?;
            if let Some(existing) = self
                .durable_set_arm
                .existing_for_operation(operation_id, &cut.anchor)?
            {
                replica.instance.authority_epoch = existing.body.successor.epoch;
                if existing.body.successor.replica != replica {
                    return Err("Conflict: original durable-set replica target differs".into());
                }
                return Ok(existing);
            }
        }
        let _admission = quiesce::acquire_after_pending_clear(
            &self.admission_gate,
            || self.durable_set_arm.pending_open(),
            deadline,
        )?;
        let cut = self.published.current()?;
        if let Some(existing) = self
            .durable_set_arm
            .existing_for_operation(operation_id, &cut.anchor)?
        {
            replica.instance.authority_epoch = existing.body.successor.epoch;
            if existing.body.successor.replica != replica {
                return Err("Conflict: original durable-set replica target differs".into());
            }
            return Ok(existing);
        }
        if !self.started || !self.state.ready.load(std::sync::atomic::Ordering::Acquire) {
            return Err("formal runtime is not ready to prepare a durable-set arm".into());
        }
        let authority = cut.anchor.authority();
        let mut successor = authority.definition().clone();
        let boundary = cut
            .anchor
            .head()
            .record()
            .position
            .checked_add(1)
            .ok_or("durable-set boundary overflow")?;
        successor.epoch = successor
            .epoch
            .checked_add(1)
            .ok_or("durable-set epoch overflow")?;
        successor.valid_from_position = boundary
            .checked_add(1)
            .ok_or("durable-set valid position overflow")?;
        successor.predecessor_authority_id =
            Some(authority.authority_id().map_err(|e| e.to_string())?);
        successor.primary.instance.authority_epoch = successor.epoch;
        replica.instance.authority_epoch = successor.epoch;
        successor.replica = replica;
        let mut body = NormalDurableSetTransitionBodyV1 {
            schema_version: 1,
            world: successor.world.clone(),
            predecessor_authority_id: authority.authority_id().map_err(|e| e.to_string())?,
            predecessor: cut.anchor.head().head(),
            boundary_position: boundary,
            operation_id: operation_id.to_owned(),
            successor,
            independent_closure_root: "0".repeat(64),
        };
        body.independent_closure_root = body.copy_contract_digest().map_err(|e| e.to_string())?;
        let transition = SignedNormalDurableSetTransitionV1 {
            writer_signature_hex: self
                .validated
                .writer_key
                .sign_durable_set_transition(&body)
                .map_err(|e| e.to_string())?,
            body,
        };
        quiesce::arm_before_deadline(deadline, || {
            self.durable_set_arm
                .arm(operation_id, transition.clone(), &cut.anchor)
        })?;
        Ok(transition)
    }

    /// Read the durable arm status against the exact currently published
    /// opaque anchor. Local arm bytes never count as a qualified commit or
    /// activation proof.
    pub(crate) fn query_durable_set_transition(
        &self,
        operation_id: &str,
        boundary_position: u64,
    ) -> Result<
        Option<crate::execution_bridge::formal_durable_set_arm::DurableSetArmStatusV1>,
        String,
    > {
        let _admission = self
            .admission_gate
            .lock()
            .map_err(|_| "formal Node admission gate poisoned")?;
        let cut = self.published.current()?;
        self.durable_set_arm
            .query(operation_id, boundary_position, &cut.anchor)
    }
}

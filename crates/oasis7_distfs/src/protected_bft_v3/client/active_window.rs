use super::*;

impl RemoteBftSignerLeaseV3 {
    pub(super) fn validate_checkpoint_history(
        &mut self,
        checkpoint: &BftSignerCheckpointV3,
    ) -> Result<(), BftSignerErrorV3> {
        if checkpoint.state.active_window.is_none() {
            super::super::state::validate_state(
                &self.binding,
                Some(&self.trust.provider_public_key),
                &checkpoint.state,
            )?;
            return Ok(());
        }
        if self
            .active_context
            .as_ref()
            .is_some_and(|c| c.validate_anchor(&checkpoint.state).is_ok())
        {
            super::super::state::validate_state_with_active_context(
                &self.binding,
                Some(&self.trust.provider_public_key),
                &checkpoint.state,
                self.active_context.clone(),
            )?;
            return Ok(());
        }
        let window = checkpoint
            .state
            .successor_window
            .as_ref()
            .ok_or_else(|| invalid("active checkpoint lacks original archives"))?;
        window.validate(&self.binding, Some(&self.trust.provider_public_key))?;
        let minimum = window
            .minimum
            .as_ref()
            .ok_or_else(|| invalid("active checkpoint lacks protected minimum"))?;
        let deadline = self.signing_result_deadline.map_or_else(
            || {
                std::time::Instant::now()
                    .checked_add(self.archive_attempt_timeout)
                    .ok_or_else(|| invalid("active archive deadline overflow"))
            },
            Ok,
        )?;
        let read_timeout = self
            .stream
            .read_timeout()
            .map_err(|e| invalid(e.to_string()))?;
        let write_timeout = self
            .stream
            .write_timeout()
            .map_err(|e| invalid(e.to_string()))?;
        let result = (|| {
            let mut chain = super::super::active_window::BftSuccessorArchiveChainVerifierV1::new();
            let mut chunks = 0;
            let mut counter = self.archive_reserved_response_bytes();
            for entry in &window.requests {
                let original = self.archive_read_state(
                    entry,
                    minimum,
                    None,
                    checkpoint,
                    deadline,
                    &mut chunks,
                    &mut counter,
                )?;
                chain.verify_next(
                    &self.binding,
                    &self.trust.provider_public_key,
                    entry,
                    &original,
                )?;
            }
            self.archive_timeout(deadline)?;
            let context = chain.context().cloned();
            super::super::state::validate_state_with_active_context(
                &self.binding,
                Some(&self.trust.provider_public_key),
                &checkpoint.state,
                context.clone(),
            )?;
            Ok(context)
        })();
        let restore = self
            .stream
            .set_read_timeout(read_timeout)
            .and_then(|_| self.stream.set_write_timeout(write_timeout));
        let context = result?;
        restore.map_err(|e| invalid(format!("restore active archive timeout: {e}")))?;
        self.active_context = context;
        Ok(())
    }
}

//! Current-lease replies carry original successor results, never a new root.
use super::*;

impl RemoteBftSignerLeaseV3 {
    pub(super) fn successor_window_result(
        &mut self,
        command: BftSignerCommandV3,
        request_id: &str,
        checkpoint_digest: &str,
    ) -> Result<
        (
            Option<BftSuccessorWindowStoredRequestV1>,
            BftSignerCheckpointV3,
        ),
        BftSignerErrorV3,
    > {
        if request_id.is_empty() || request_id.len() > 256 || request_id.trim() != request_id {
            return Err(invalid("successor request identity bounds"));
        }
        super::super::state::validate_digest(checkpoint_digest)?;
        let (request, reply) = self.command(command)?;
        RemoteBftSignerProviderV3::checked_result(&reply)?;
        let BftSignerReplyKindV3::SuccessorWindow { record } = &reply.body.result else {
            return Err(invalid("successor response has the wrong result kind"));
        };
        let checkpoint = self.verified_checkpoint(&request, &reply, self.credential.generation)?;
        if checkpoint.state.revision < self.checkpoint.state.revision
            || (checkpoint.state.revision == self.checkpoint.state.revision
                && checkpoint.state.state_digest != self.checkpoint.state.state_digest)
        {
            return Err(invalid(
                "successor reply regressed or forked the observed protected state",
            ));
        }
        if let Some(previous) = self
            .checkpoint
            .state
            .successor_window
            .as_ref()
            .and_then(|state| state.minimum.as_ref())
        {
            checkpoint
                .state
                .successor_window
                .as_ref()
                .and_then(|state| state.minimum.as_ref())
                .ok_or_else(|| invalid("successor reply removed the observed minimum"))?
                .validate_after(previous)?;
        }
        let stored = checkpoint
            .state
            .successor_window
            .as_ref()
            .and_then(|state| state.request(request_id));
        if stored != record.as_deref() {
            return Err(invalid(
                "successor reply differs from its protected checkpoint record",
            ));
        }
        if stored.is_none()
            && checkpoint
                .state
                .successor_window
                .as_ref()
                .and_then(|state| state.reservation.as_ref())
                .is_some_and(|reservation| reservation.body.checkpoint.request_id == request_id)
        {
            return Err(invalid(
                "successor original reservation is unresolved, not absent",
            ));
        }
        if let Some(record) = stored {
            record.validate(&self.binding, &self.trust.provider_public_key)?;
            let intent = &record.request().intent;
            if intent.body.checkpoint.request_id != request_id
                || intent.body.checkpoint.digest()? != checkpoint_digest
                || intent.body.expected_lease_generation > self.credential.generation
                || record.prepared_revision > checkpoint.state.revision
                || record.request().receipt.as_ref().is_some_and(|receipt| {
                    receipt.body.lease_generation > self.credential.generation
                        || receipt.body.committed_revision > checkpoint.state.revision
                })
            {
                return Err(invalid(
                    "successor reply changed its original request/checkpoint",
                ));
            }
        }
        Ok((record.as_deref().cloned(), checkpoint))
    }
}

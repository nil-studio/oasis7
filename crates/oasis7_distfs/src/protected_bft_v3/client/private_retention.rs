//! Explicit externally configured retention role; receipt DTOs cannot choose trust.
use super::*;
use crate::protected_bft_v3::private_retention::*;
use crate::protected_bft_v3::private_retention_backend::BftPrivateRetentionLedgerV1;
use std::time::Instant;

pub(super) fn validate_head_role(
    state: &super::super::state::BftSignerStateV3,
    role: Option<&BftPrivateRetentionAttestorV1>,
    binding: &BftSignerBindingV3,
    provider: &str,
) -> Result<(), BftSignerErrorV3> {
    let Some(head) = &state.private_retention else {
        return Ok(());
    };
    let role = role.ok_or_else(|| {
        invalid("private retention State requires explicit external attestor trust")
    })?;
    validate_role(role, binding, provider)?;
    if head.capability_digest != role.digest()? || head.source != role.source {
        return Err(invalid(
            "private retention head differs from external capability",
        ));
    }
    Ok(())
}

fn validate_role(
    role: &BftPrivateRetentionAttestorV1,
    binding: &BftSignerBindingV3,
    provider: &str,
) -> Result<(), BftSignerErrorV3> {
    role.digest()?;
    if role.provider_public_key != provider
        || role.source.binding_digest != binding.digest()?
        || role.source.handoff_digest != binding.handoff_record_id
        || role.source.material_source_digest != binding.material_identity
        || role.coverage_start_height != binding.initial_schedule().height()
    {
        return Err(invalid(
            "private retention external role source/provider mismatch",
        ));
    }
    Ok(())
}

impl RemoteBftSignerProviderV3 {
    pub fn with_private_retention_attestor(
        mut self,
        role: BftPrivateRetentionAttestorV1,
    ) -> Result<Self, BftSignerErrorV3> {
        validate_role(&role, &self.binding, &self.trust.provider_public_key)?;
        self.private_retention_attestor = Some(role);
        Ok(self)
    }
}

impl RemoteBftSignerLeaseV3 {
    pub(super) fn private_retention_result(
        &mut self,
        command: BftSignerCommandV3,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionLedgerV1, BftSignerErrorV3> {
        let role = self
            .private_retention_attestor
            .clone()
            .ok_or_else(|| invalid("private retention explicit trust unavailable"))?;
        let expected_seal = match &command {
            BftSignerCommandV3::SealPrivateRetention {
                expected_revision,
                expected_state_digest,
                cut,
            } => Some((
                *expected_revision,
                expected_state_digest.clone(),
                cut.as_ref().clone(),
            )),
            _ => None,
        };
        let expected_input = match &command {
            BftSignerCommandV3::ReservePrivateRetentionInput { reference, .. } => {
                Some(reference.as_ref().clone())
            }
            _ => None,
        };
        let expected_selection = match &command {
            BftSignerCommandV3::ReadyPrivateRetentionOriginalLocal { key, .. } => Some((
                key.as_ref().clone(),
                None,
                Some(crate::protected_bft_v3::BftPrivateRetentionSelectionKindV1::OriginalLocal),
                None,
            )),
            BftSignerCommandV3::SelectPrivateRetention { selection } => Some((
                selection.key.clone(),
                Some(selection.reference.clone()),
                None,
                Some(selection.digest()?),
            )),
            _ => None,
        };
        let expected = match &command {
            BftSignerCommandV3::ReservePrivateRetention { entry, .. } => {
                Some((entry.key.clone(), Some(entry.fact.clone()), false))
            }
            BftSignerCommandV3::ReservePrivateRetentionInput { reference, .. } => {
                Some((reference.key.clone(), None, false))
            }
            BftSignerCommandV3::ReadyPrivateRetention { key, .. }
            | BftSignerCommandV3::ReadyPrivateRetentionOriginalLocal { key, .. } => {
                Some((key.as_ref().clone(), None, true))
            }
            _ => None,
        };
        if Instant::now() >= deadline {
            return Err(invalid("private retention client attempt expired"));
        }
        let prior_deadline = self.signing_result_deadline;
        let read_timeout = self
            .stream
            .read_timeout()
            .map_err(|e| invalid(e.to_string()))?;
        let write_timeout = self
            .stream
            .write_timeout()
            .map_err(|e| invalid(e.to_string()))?;
        self.signing_result_deadline =
            Some(prior_deadline.map_or(deadline, |old| old.min(deadline)));
        let result = (|| {
            let previous_revision = self.checkpoint.state.revision;
            let previous_digest = self.checkpoint.state.state_digest.clone();
            let (request, reply) = self.command(command)?;
            RemoteBftSignerProviderV3::checked_result(&reply)?;
            let BftSignerReplyKindV3::PrivateRetention {
                ledger,
                latest_seal,
                reserved_response_bytes,
            } = &reply.body.result
            else {
                return Err(invalid("private retention reply kind"));
            };
            let checkpoint =
                self.verified_checkpoint(&request, &reply, self.credential.generation)?;
            super::super::archive_transfer::validate_successor_archive_budget(
                &checkpoint.state,
                Some(
                    &super::super::archive_transfer::BftSuccessorArchiveTransferLedgerV1 {
                        schema_version: 1,
                        reserved_response_bytes: *reserved_response_bytes,
                    },
                ),
            )?;
            super::super::private_retention_current_anchor::verify_current_anchor(
                &role,
                ledger,
                latest_seal.as_deref(),
            )?;
            let body = &ledger.receipt.body;
            if checkpoint.state.private_retention.as_ref() != Some(&body.head)
                || checkpoint.state.revision < body.after_revision
                || (checkpoint.state.revision == body.after_revision
                    && checkpoint.state.state_digest != body.after_state_digest)
                || checkpoint.state.revision < previous_revision
                || (checkpoint.state.revision == previous_revision
                    && checkpoint.state.state_digest != previous_digest)
                || body.original_lease_generation > self.credential.generation
                || *reserved_response_bytes <= self.archive_reserved_response_bytes()
            {
                return Err(invalid(
                    "private retention reply regressed or changed protected binding",
                ));
            }
            if let Some((revision, digest, cut)) = expected_seal {
                let anchor = latest_seal
                    .as_deref()
                    .ok_or_else(|| invalid("seal reply omitted original compact anchor"))?;
                if anchor.body.cut != cut
                    || body.before_revision != revision
                    || body.before_state_digest != digest
                    || checkpoint.state.revision != body.after_revision
                    || checkpoint.state.state_digest != body.after_state_digest
                    || ledger.map.entries != anchor.body.carry_entries
                {
                    return Err(invalid(
                        "seal reply differs from original cut/post-State/carry",
                    ));
                }
            }
            if let Some((key, reference, kind, operation)) = expected_selection {
                let selected = ledger
                    .selection_index
                    .as_ref()
                    .and_then(|i| i.selections.iter().find(|s| s.key == key))
                    .ok_or_else(|| invalid("selection reply omitted original choice"))?;
                if reference.as_ref().is_some_and(|r| r != &selected.reference)
                    || kind.is_some_and(|k| k != selected.kind)
                    || operation
                        .as_ref()
                        .is_some_and(|op| op != &selected.operation_digest)
                    || selected.operation.digest()? != selected.operation_digest
                {
                    return Err(invalid("selection reply changed original typed choice"));
                }
            }
            if let Some((key, fact, require_ready)) = expected {
                let entry = ledger
                    .map
                    .entries
                    .binary_search_by(|entry| entry.key.cmp(&key))
                    .ok()
                    .map(|index| &ledger.map.entries[index])
                    .ok_or_else(|| invalid("private retention reply omitted requested entry"))?;
                if let Some(reference) = &expected_input {
                    let actual = super::super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1::from_entry(&ledger.map.source, entry)?;
                    if actual != *reference {
                        return Err(invalid(
                            "private retention reply changed original input reference",
                        ));
                    }
                }
                if fact.as_ref().is_some_and(|fact| fact != &entry.fact)
                    || (require_ready && entry.lifecycle != BftPrivateRetentionLifecycleV1::Ready)
                {
                    return Err(invalid(
                        "private retention reply changed original fact/lifecycle",
                    ));
                }
            }
            if Instant::now() >= deadline {
                return Err(invalid("private retention client attempt expired"));
            }
            self.private_retention_latest_seal = latest_seal.as_deref().cloned();
            self.private_retention_transfer_floor = *reserved_response_bytes;
            self.checkpoint = checkpoint;
            Ok(ledger.as_ref().clone())
        })();
        self.signing_result_deadline = prior_deadline;
        let restored = self
            .stream
            .set_read_timeout(read_timeout)
            .and_then(|()| self.stream.set_write_timeout(write_timeout))
            .map_err(|e| invalid(e.to_string()));
        restored.and(result)
    }
}

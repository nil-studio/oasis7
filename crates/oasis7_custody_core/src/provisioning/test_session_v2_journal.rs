//! Account transaction order inside one genuine V2 authority, never a new grant.
use super::*;
#[path = "test_session_v2_journal_replay.rs"]
mod replay;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum SessionJournalProfileV2 {
    Credential,
    Control,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::provisioning) enum SessionEventKindV2 {
    NativeModelHttpSettled {
        ingress_attempt_id: String,
    },
    NativeModelGraphReserved {
        reservation_id: String,
    },
    PreflightReservation,
    ApprovalVerificationClaim,
    ApprovalConsumed,
    SourceReserved {
        source_operation_id: String,
    },
    SourceReadReserved {
        source_operation_id: String,
        ticket: u8,
    },
    SourceVerificationClaimed {
        source_operation_id: String,
    },
    SourceVerified {
        source_operation_id: String,
    },
    PreflightClaimed,
    PreflightReadReserved {
        ticket: u8,
    },
    PreflightWorldAccessReserved,
    PreflightWorldAccessAdmitted,
    PreflightAccessRecorded,
    PreflightLifetimeRecorded,
    PreflightReadPublished,
    PreflightStep {
        step: SessionPreflightStepV1,
    },
    Activated,
    NativeMachineActivationClaimed,
    NativeMachineActivationAdmitted,
    ClientHttpCharged {
        ingress_attempt_id: String,
    },
    LifetimeClaimed {
        source_operation_id: String,
        ingress_attempt_id: String,
        profile: SessionJournalProfileV2,
    },
    ChallengeIssued {
        request_id: String,
        source_operation_id: String,
        ingress_attempt_id: String,
        profile: SessionJournalProfileV2,
    },
    CredentialClaimed {
        request_id: String,
    },
    ControlAcknowledged {
        request_id: String,
    },
    Expired {
        reason: TestSessionClosingReasonV1,
    },
    // Closed consumers remain unsupported until their actual records, bounds
    // and replay delegation are composed; these variants confer no authority.
    PurposeHttpCharged {
        ingress_attempt_id: String,
    },
    ToolMetadataNonceIssued {
        request_id: String,
    },
    ExecutionPrepareFactoryPaid {
        request_id: String,
    },
    ExecutionPrepareCurrentPortsClaimed {
        request_id: String,
    },
    ExecutionPrivateCurrentClaimed {
        request_id: String,
    },
    ExecutionPrivateCurrentObserved {
        request_id: String,
    },
    ExecutionNativeGateReserved {
        request_id: String,
    },
    ExecutionNativeDispatchReceiptRecorded {
        request_id: String,
        edge: crate::native_execution_prepare_v2::ExecutionPreparePhysicalEdgeV2,
    },
    ExecutionNativeDispatchClaimed {
        request_id: String,
        edge: crate::native_execution_prepare_v2::ExecutionPreparePhysicalEdgeV2,
    },
    ExecutionNativePrepareClaimed {
        request_id: String,
    },
    ExecutionPreparedPublicArchived {
        request_id: String,
    },
    ExecutionPrepareOriginalArchived {
        request_id: String,
    },
    ExecutionPreparePermitsRecorded {
        request_id: String,
    },
    ExecutionPrepareAllocated {
        request_id: String,
    },
    ExecutionBookStarted {
        request_id: String,
    },
    ExecutionLifetimeClaimed {
        request_id: String,
        source_operation_id: String,
    },
    ExecutionLifetimeArchived {
        request_id: String,
        source_operation_id: String,
    },
    ExecutionLifetimeAdmitted {
        request_id: String,
        source_operation_id: String,
    },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct SessionEventV2 {
    pub(super) event: SessionEventKindV2,
    pub(super) at: UnixMillis,
}
impl SessionEventV2 {
    pub(in crate::provisioning) fn event(&self) -> &SessionEventKindV2 {
        &self.event
    }
}
impl core::fmt::Debug for SessionEventV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SessionEventV2(<reference>)")
    }
}
/// Existing bounded inventories determine the count. No independent log cap or
/// unlimited retry append may manufacture inventory/transport authority.
fn inventory_count(s: &TestSessionAuthorityV2) -> Result<usize> {
    let mut n = 0usize;
    for add in [
        usize::from(s.book.original(&s.input.operation_id).is_some()),
        usize::from(s.approval_verification_claimed_at.is_some()),
        usize::from(s.consumed_at.is_some()),
        usize::from(s.preflight_claimed_at.is_some()),
        usize::from(s.preflight_world_access.is_some()),
        usize::from(
            s.preflight_world_access
                .as_ref()
                .is_some_and(|w| w.admitted()),
        ),
        s.preflight_steps.len(),
        usize::from(s.approved_at.is_some()),
        usize::from(
            s.native_machine_registration
                .as_ref()
                .is_some_and(|m| m.active.is_some()),
        ),
        usize::from(
            s.native_machine_registration
                .as_ref()
                .is_some_and(|m| m.ready()),
        ),
        s.sources.len(),
        s.client_http_admissions.len(),
        s.purpose_http_admissions.len(),
        s.native_model_budget
            .as_ref()
            .map_or(0, |m| m.http_admissions.len()),
        s.native_model_budget
            .as_ref()
            .map_or(0, |m| m.reservations.len()),
        s.execution_book.as_ref().map_or(0, |book| {
            let b = book.journal_record();
            2 + usize::from(b.lifetime_archived_at.is_some())
                + usize::from(b.lifetime_admitted_at.is_some())
                + book.prepare_stage_count()
                + book.prepare_paid_count()
                + usize::from(book.prepare_current_read(s).is_some())
                + usize::from(book.prepare_service_record().is_some())
                + usize::from(book.prepare_original_recorded_at().is_some())
                + usize::from(book.native_prepare_record().is_some())
                + book
                    .native_prepare_record()
                    .and_then(|r| r.dispatch_gate.as_ref())
                    .map_or(0, |g| {
                        1 + g.claims.len()
                            + g.claims
                                .iter()
                                .filter(|c| c.signature_recorded_at.is_some())
                                .count()
                    })
                + usize::from(
                    book.native_prepare_record()
                        .is_some_and(|r| r.public_recorded_at.is_some()),
                )
                + usize::from(book.private_current_read(s).is_some())
                + usize::from(
                    book.private_current_read(s)
                        .is_some_and(|r| r.completed_at.is_some()),
                )
        }),
        s.control_transports
            .values()
            .filter(|h| **h == SessionControlHopV1::Identity)
            .count(),
        s.challenges.len(),
        s.credential_claims.len(),
        s.controls.len(),
        usize::from(s.closing.is_some_and(|(r, _)| {
            matches!(
                r,
                TestSessionClosingReasonV1::Deadline | TestSessionClosingReasonV1::LeaseExpired
            )
        })),
    ] {
        n = n.checked_add(add).ok_or_else(invalid)?;
    }
    for wrapper in s.sources.values() {
        n = n
            .checked_add(usize::from(wrapper.source.claimed_at.is_some()))
            .ok_or_else(invalid)?;
        n = n
            .checked_add(usize::from(wrapper.source.verification.is_some()))
            .ok_or_else(invalid)?;
    }
    for count in s.source_oracle_reads.values() {
        n = n.checked_add(usize::from(*count)).ok_or_else(invalid)?;
    }
    for add in [
        usize::from(s.preflight_oracle_reads),
        usize::from(s.preflight_access.is_some()),
        usize::from(s.preflight_lifetime.is_some()),
        usize::from(s.preflight_read.is_some()),
    ] {
        n = n.checked_add(add).ok_or_else(invalid)?;
    }
    Ok(n)
}
pub(in crate::provisioning) fn session_event_ordinal_v2(
    s: &TestSessionAuthorityV2,
    kind: &SessionEventKindV2,
) -> Result<Option<u64>> {
    let mut found = None;
    for (i, event) in s.events.iter().enumerate() {
        if &event.event == kind {
            if found.is_some() {
                return Err(invalid());
            }
            found = Some(
                u64::try_from(i)
                    .map_err(|_| invalid())?
                    .checked_add(1)
                    .ok_or_else(invalid)?,
            );
        }
    }
    Ok(found)
}
pub(in crate::provisioning) fn next_session_ordinal_v2(s: &TestSessionAuthorityV2) -> Result<u64> {
    u64::try_from(s.events.len())
        .map_err(|_| invalid())?
        .checked_add(1)
        .ok_or_else(invalid)
}
pub(in crate::provisioning) fn append_session_event_v2(
    s: &mut TestSessionAuthorityV2,
    event: SessionEventKindV2,
    at: UnixMillis,
) -> Result<u64> {
    let count = inventory_count(s)?;
    if session_event_ordinal_v2(s, &event)?.is_some()
        || s.events.len().checked_add(1).is_none_or(|n| n > count)
    {
        return Err(invalid());
    }
    let ordinal = next_session_ordinal_v2(s)?;
    s.events.push(SessionEventV2 { event, at });
    Ok(ordinal)
}
pub(in crate::provisioning) fn validate(s: &TestSessionAuthorityV2) -> Result<()> {
    if s.events.len() != inventory_count(s)? {
        return Err(invalid());
    }
    replay::validate(s)
}

//! Exact reverse of paid pending metadata; no historical token constructor.
use super::*;
pub(in crate::provisioning) fn validate_pending_start_v2(s: &TestSessionAuthorityV2) -> Result<()> {
    if s.execution_pending_ingresses.len() > 80 {
        return Err(invalid());
    }
    let mut ids = std::collections::BTreeSet::new();
    for (ingress, original) in &s.execution_pending_ingresses {
        let (plan, scope) = s.header()?;
        if ingress.is_empty()
            || ingress.len() > 256
            || ingress.chars().any(char::is_control)
            || original.request.api_version != 2
            || original.request.session_id != scope.session_id
            || original.approved_input_digest != s.input_digest()?
            || !hex64(&original.signed_attempt_digest)
            || original.credential_generation.0 == 0
            || original.charged_at.get() >= original.signed_until.get()
            || original.signed_until.get()
                > s.roots
                    .get(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .deadline
                    .get()
            || !plan.selection.is_exact_move0()
            || plan.business_limit.0 != 0
            || !plan.allowed_resource_refs.is_empty()
            || !matches!((&original.request.action,plan.actions.as_slice()),(SessionToolActionV1::Execute(a),[b]) if a==b)
        {
            return Err(invalid());
        }
        let c = s
            .challenges
            .get(&original.challenge_request_id)
            .ok_or_else(invalid)?;
        let link = s
            .tool_nonce_links
            .get(&original.challenge_request_id)
            .ok_or_else(invalid)?;
        let metadata = s
            .purpose_http_admissions
            .get(&link.first_ingress)
            .ok_or_else(invalid)?;
        if c.kind != SessionChallengeKindV1::ToolMetadata
            || c.purpose != SessionPurposeV1::Execution
            || c.logical_request_id != original.request.request_id
            || c.body_digest != original.request.digest()?
            || c.credential_generation != original.credential_generation
            || original.charged_at.get() < c.created_at.get()
            || original.charged_at.get() >= c.expires_at.get()
            || original.signed_until.get() > c.expires_at.get()
            || !matches!(
                metadata.class,
                SessionPurposeHttpAdmissionClassV1::ExecutionNonceFiveV2 { .. }
            )
        {
            return Err(invalid());
        }
        let paid = s.purpose_http_admissions.get(ingress).ok_or_else(invalid)?;
        if paid.class != fee_class(original)
            || paid.purpose != SessionPurposeV1::Execution
            || paid.hop != SessionControlHopV1::Host
            || paid.signed_attempt_digest != original.signed_attempt_digest
            || paid.charged_at != original.charged_at
            || !ids.insert(ingress.clone())
        {
            return Err(invalid());
        }
        for (kind, key) in [
            StartIngressPortV2::AbuseProviderActor,
            StartIngressPortV2::AbuseRealmActor,
            StartIngressPortV2::OwnerService,
            StartIngressPortV2::AccountActor,
        ]
        .into_iter()
        .zip(&original.ingress_ports)
        {
            if *key != port_id(s, ingress, original, &ExecutionPendingLeafV2::Ingress(kind))?
                || !ids.insert(key.clone())
                || s.purpose_transports.get(key) != Some(&SessionPurposeV1::Execution.into())
                || s.sources.contains_key(key)
                || s.sources.values().any(|v| v.source.verify_nonce == *key)
                || s.control_transports.contains_key(key)
                || s.client_http_admissions.contains_key(key)
                || s.tool_nonce_ingresses.contains_key(key)
                || s.termination_state.transport_id_in_use(key)
                || s.execution_book
                    .as_ref()
                    .is_some_and(|b| b.transport_id_in_use(key))
            {
                return Err(invalid());
            }
        }
    }
    // A new class without its actual parent is never an ordinary fee admission.
    for (key, fee) in &s.purpose_http_admissions {
        if matches!(
            fee.class,
            SessionPurposeHttpAdmissionClassV1::ExecutionStartFiveV2 { .. }
        ) && !s.execution_pending_ingresses.contains_key(key)
        {
            return Err(invalid());
        }
    }
    let Some(p) = &s.execution_pending_start else {
        return Ok(());
    };
    let original = s
        .execution_pending_ingresses
        .get(&p.ingress_attempt_id)
        .ok_or_else(invalid)?;
    let c = s
        .challenges
        .get(&original.challenge_request_id)
        .ok_or_else(invalid)?;
    let bound = p
        .accepted_at
        .get()
        .checked_add(5000)
        .ok_or_else(invalid)?
        .min(original.signed_until.get())
        .min(c.expires_at.get())
        .min(p.accepted_lease_until.get())
        .min(
            s.roots
                .get(&SessionPurposeV1::Execution)
                .ok_or_else(invalid)?
                .deadline
                .get(),
        );
    if !hex64(&p.source_nonce_salt)
        || p.accepted_at.get() < original.charged_at.get()
        || p.absolute_until.get() != bound
        || p.accepted_at.get() >= bound
        || (s.execution_book.is_none() && c.consumed_by.is_some())
        || p.accepted_ordinal == 0
        || p.physical_ports.len() != 134
        || session_event_ordinal_v2(
            s,
            &SessionEventKindV2::ExecutionPendingReserved {
                request_id: original.request.request_id.clone(),
            },
        )? != Some(p.accepted_ordinal)
    {
        return Err(invalid());
    }
    let source_id = format!(
        "execution-start-source-{}",
        digest(&(
            "oasis7.custody.execution-pending-source-id.v2",
            &original.approved_input_digest,
            &original.request.session_id,
            &p.ingress_attempt_id,
            &original.signed_attempt_digest,
            &original.request,
            &p.source_nonce_salt
        ))?
    );
    let nonce = digest(&(
        "oasis7.custody.execution-pending-source-nonce.v2",
        &original.approved_input_digest,
        &original.request.session_id,
        &source_id,
        &p.ingress_attempt_id,
        &original.signed_attempt_digest,
        original.request.digest()?,
        &p.source_nonce_salt,
    ))?;
    if source_id != p.source_operation_id
        || nonce != p.source_verify_nonce
        || nonce == p.source_nonce_salt
        || !ids.insert(source_id)
        || !ids.insert(nonce)
    {
        return Err(invalid());
    }
    let source = s.sources.get(&p.source_operation_id).ok_or_else(invalid)?;
    if source.observation_profile.is_some()
        || source.client_admission.is_some()
        || !pending_funds_source_v2(s, &p.source_operation_id)
        || source.approved_input_digest != original.approved_input_digest
        || source.source.operation_id != p.source_operation_id
        || source.source.verify_nonce != p.source_verify_nonce
        || source.source.created_at.0 != p.accepted_at.get()
        || source.source.expires_at.0 != p.absolute_until.get()
    {
        return Err(invalid());
    }
    for (expected, port) in inventory::fixed_inventory()?
        .into_iter()
        .skip(5)
        .zip(&p.physical_ports)
    {
        if port.claimed_ordinal.is_some() != port.claimed_at.is_some() {
            return Err(invalid());
        }
        if let Some(ordinal) = port.claimed_ordinal {
            if !fence::first_rf_sequence().contains(&port.leaf)
                || ordinal <= p.accepted_ordinal
                || port.claimed_at.is_none_or(|at| {
                    at.get() < p.accepted_at.get() || at.get() >= p.absolute_until.get()
                })
                || session_event_ordinal_v2(
                    s,
                    &SessionEventKindV2::ExecutionPendingLeafClaimed {
                        request_id: original.request.request_id.clone(),
                        leaf: port.leaf,
                    },
                )? != Some(ordinal)
            {
                return Err(invalid());
            }
        }
        if port.leaf != expected
            || port.physical_id != port_id(s, &p.ingress_attempt_id, original, &expected)?
            || !ids.insert(port.physical_id.clone())
            || s.sources.contains_key(&port.physical_id)
            || s.sources
                .values()
                .any(|v| v.source.verify_nonce == port.physical_id)
            || s.purpose_transports.contains_key(&port.physical_id)
            || s.purpose_http_admissions.contains_key(&port.physical_id)
            || s.control_transports.contains_key(&port.physical_id)
            || s.client_http_admissions.contains_key(&port.physical_id)
            || s.tool_nonce_ingresses.contains_key(&port.physical_id)
            || s.termination_state.transport_id_in_use(&port.physical_id)
            || s.execution_book
                .as_ref()
                .is_some_and(|b| b.transport_id_in_use(&port.physical_id))
        {
            return Err(invalid());
        }
    }
    if let Some(record) = &p.directory_signature {
        let expected = directory_signature::expected_claim(s, record.proof.claim.issuer.clone())?;
        if expected != record.proof.claim
            || record.recorded_at.get() < record.proof.claim.issued_at.0
            || session_event_ordinal_v2(
                s,
                &SessionEventKindV2::ExecutionPendingDirectorySignatureRecorded {
                    request_id: original.request.request_id.clone(),
                },
            )?
            .is_none()
        {
            return Err(invalid());
        }
        record.proof.verify_issuer(&record.pin,&record.floor,crate::execution_pending_current_v2::PendingCurrentReceiverV2::ManagerAssociationsActor,record.proof.claim.issued_at.0).map_err(|_|invalid())?;
    }
    fence::validate_records(s)?;
    Ok(())
}

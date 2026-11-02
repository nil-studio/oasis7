//! Exact reverse/strict restore for the independent Read producer records.
use super::*;
pub(in crate::provisioning) fn validate_read_current_v3(
    state: &AccountProvisionApprovalState,
    s: &TestSessionAuthorityV2,
) -> Result<()> {
    let id = &s.input.operation_id;
    let input = s.input_digest()?;
    let mut routes = BTreeSet::new();
    for (ingress, a) in &s.purpose_http_admissions {
        let SessionPurposeHttpAdmissionClassV1::ReadCurrentFiveV3 { preparation: r } = &a.class
        else {
            continue;
        };
        let root = s
            .roots
            .get(&SessionPurposeV1::ReconcileRead)
            .ok_or_else(invalid)?;
        let c = s
            .challenges
            .get(&r.challenge_request_id)
            .ok_or_else(invalid)?;
        let expected: [String; 4] = [
            "provider_abuse",
            "realm_abuse",
            "owner_service",
            "account_actor",
        ]
        .map(|route| digest(&("oasis7.custody.read-current-ingress.v3", id, ingress, route)))
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| invalid())?;
        if a.purpose != SessionPurposeV1::ReconcileRead
            || a.hop != SessionControlHopV1::Host
            || a.charged_at != r.settled_at
            || r.request.session_id != *id
            || r.request.purpose() != SessionPurposeV1::ReconcileRead
            || r.settled_at.get() < s.approved_at.ok_or_else(invalid)?.get()
            || r.settled_at.get() >= r.request_until.get()
            || r.request_until.get() > root.deadline.get()
            || r.credential_generation.0 == 0
            || r.credential_generation.0
                > root.credential.as_ref().ok_or_else(invalid)?.generation.0
            || r.entry_routes != expected
            || expected.iter().any(|x| {
                !routes.insert(x.clone())
                    || s.purpose_http_admissions.contains_key(x)
                    || super::super::v2_purpose_http::purpose_http_occupied_except_self_v2(
                        s, x, false,
                    )
                    || s.native_model_budget
                        .as_ref()
                        .is_some_and(|m| m.id_in_use(x))
                    || s.preflight_world_access
                        .as_ref()
                        .is_some_and(|w| w.transport_id_in_use(x))
            })
            || c.purpose != SessionPurposeV1::ReconcileRead
            || c.kind != SessionChallengeKindV1::ToolMetadata
            || c.logical_request_id != r.request.request_id
            || c.body_digest != r.request.digest()?
            || c.consumed_by.as_deref() != Some(r.request.request_id.as_str())
            || (r.consumed_challenge
                && (c.expires_at.get() <= r.settled_at.get()
                    || c.credential_generation != r.credential_generation))
            || session_event_ordinal_v2(
                s,
                &SessionEventKindV2::ReadCurrentIngressSettled {
                    ingress_attempt_id: ingress.clone(),
                },
            )?
            .is_none()
        {
            return Err(invalid());
        }
        if let Some(expected) = &r.original_digest {
            if known(state, id, &r.request)?.as_ref() != Some(expected) {
                return Err(invalid());
            }
        }
        let source = source_id(id, ingress, &input)?;
        let reserved = session_event_ordinal_v2(
            s,
            &SessionEventKindV2::ReadCurrentReserved {
                ingress_attempt_id: ingress.clone(),
            },
        )?;
        match &r.reservation {
            None => {
                if reserved.is_some() || s.sources.contains_key(&source) {
                    return Err(invalid());
                }
            }
            Some(p) => {
                let w = s.sources.get(&p.source_id).ok_or_else(invalid)?;
                if r.original_digest.is_none()
                    || reserved.is_none()
                    || p.source_id != source
                    || !hex64(&p.nonce)
                    || p.reserved_at.get() < r.settled_at.get()
                    || p.reserved_at.get() >= p.until.get()
                    || p.until.get() > r.request_until.get()
                    || p.until.get() > root.deadline.get()
                    || p.tickets > 5
                    || p.graph_digest != graph(&input, ingress, &p.source_id, &p.nonce, p.until)?
                    || s.source_funding.get(&p.source_id)
                        != Some(&SessionSourceFundingV1::ReconcileRead)
                    || w.api_version != 2
                    || w.approved_input_digest != input
                    || w.source.operation_id != p.source_id
                    || w.source.verify_nonce != p.nonce
                    || w.source.created_at.0 != p.reserved_at.get()
                    || w.source.expires_at.0
                        != p.reserved_at
                            .get()
                            .checked_add(30_000)
                            .ok_or_else(invalid)?
                            .min(root.deadline.get())
                    || s.source_oracle_reads.get(&p.source_id) != Some(&p.tickets)
                    || w.source.claimed_at.is_some()
                    || w.source.verified_at.is_some()
                {
                    return Err(invalid());
                }
                for (port, claim) in &p.claims {
                    let event = SessionEventKindV2::ReadCurrentPortPrepaidClaimed {
                        ingress_attempt_id: ingress.clone(),
                        port: *port,
                    };
                    if claim.at.get() < p.reserved_at.get()
                        || claim.at.get() >= p.until.get()
                        || session_event_ordinal_v2(s, &event)? != Some(claim.ordinal)
                        || s.events
                            .get(
                                usize::try_from(claim.ordinal.checked_sub(1).ok_or_else(invalid)?)
                                    .map_err(|_| invalid())?,
                            )
                            .is_none_or(|e| e.at != claim.at)
                    {
                        return Err(invalid());
                    }
                }
            }
        }
    }
    Ok(())
}
pub(in crate::provisioning) fn read_current_entry_internal_v3(
    s: &TestSessionAuthorityV2,
    id: &str,
) -> bool {
    s.purpose_http_admissions.values().any(|a| match &a.class {
        SessionPurposeHttpAdmissionClassV1::ReadCurrentFiveV3 { preparation } => {
            preparation.entry_routes.iter().any(|x| x == id)
        }
        _ => false,
    })
}
pub(in crate::provisioning) fn read_current_nonce_consumed_v3(
    s: &TestSessionAuthorityV2,
    c: &SessionChallengeV1,
) -> bool {
    s.purpose_http_admissions.iter().any(|(id,a)|match &a.class {
        SessionPurposeHttpAdmissionClassV1::ReadCurrentFiveV3 {preparation:r}=>
            a.purpose==SessionPurposeV1::ReconcileRead&&a.hop==SessionControlHopV1::Host
            &&a.charged_at==r.settled_at&&r.challenge_request_id==c.request_id&&r.consumed_challenge
            &&c.purpose==SessionPurposeV1::ReconcileRead&&c.kind==SessionChallengeKindV1::ToolMetadata
            &&c.logical_request_id==r.request.request_id&&c.body_digest==r.request.digest().unwrap_or_default()
            &&c.consumed_by.as_deref()==Some(r.request.request_id.as_str())
            &&c.credential_generation==r.credential_generation
            &&s.events.iter().any(|e| e.at==r.settled_at && matches!(&e.event,SessionEventKindV2::ReadCurrentIngressSettled {ingress_attempt_id} if ingress_attempt_id==id)),
        _=>false,
    })
}

use super::*;
use oasis7_runtime::{WorldEvent, WorldEventBody};

pub(super) fn describe(
    service: &Service<'_>,
    request: SignedReadRequest<DescribeWorldRequest>,
) -> Result<Value, String> {
    verify_read_request(DESCRIBE_PATH, &request)?;
    request.request.validate().map_err(|e| e.to_string())?;
    if request.request.expected_world != service.identity
        || request.request.trust_config_ref != service.signer.public_key_hex
    {
        return Err("service world or configured trust mismatch".into());
    }
    let pinned = service.pin(None)?;
    let version = version(&pinned, "public", &json!(null))?;
    let mut capabilities = vec![
        Capability::StableLookup,
        Capability::IdempotentRetry,
        Capability::CommittedView,
        Capability::MinimumCommit,
        Capability::FixedHistoricalView,
        Capability::BoundedChanges,
    ];
    if service.guarded.is_some() {
        capabilities.push(Capability::GovernanceMembership);
    }
    service.signed(
        DESCRIBE_PATH,
        &request,
        DescribeWorldResponse {
            formal_profile: None,
            execution_evidence_scope: service.guarded.map(|_| {
                oasis7::world_service::verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE.into()
            }),
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: service.identity.clone(),
            binding: pinned.commit.binding.clone(),
            capabilities,
            availability: WorldAvailability {
                readable: true,
                writable: true,
                recovering: false,
                reason: None,
                retry_after_ms: None,
            },
            current_view: Some(version),
        },
    )
}

fn outcome(
    service: &Service<'_>,
    pinned: &PinnedWorld,
    correlation: &RequestCorrelation,
) -> Result<IntentOutcome<Value>, String> {
    let key = correlation::key_digest(&correlation.key)?;
    let Some(value) = pinned
        .world
        .capability_revocation_state()
        .world_service_results
        .get(&key)
    else {
        return Ok(IntentOutcome::Unknown);
    };
    correlation::validate_result(&key, value)?;
    let result: CanonicalIntentResultV1 =
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    if result.request.correlation != *correlation {
        return Err("request key already has different signed payload".into());
    }
    if result.rejected.is_some() {
        return Ok(IntentOutcome::Rejected {
            reason: WorldServiceErrorKind::Conflict,
        });
    }
    // A compacted header does not retain the immutable execution material.
    let historical = if service.guarded.is_some() {
        service.pin_at_height(result.committed_height)?
    } else {
        let Some(historical) = world_service_read::pin_retained_at_height(
            service.records,
            service.storage,
            &service.identity,
            result.committed_height,
        )?
        else {
            return Ok(IntentOutcome::HistoryUnavailable);
        };
        historical
    };
    if historical
        .world
        .capability_revocation_state()
        .world_service_results
        .get(&key)
        != Some(value)
    {
        return Err("historical snapshot does not bind canonical result".into());
    }
    Ok(IntentOutcome::Committed {
        commit: Box::new(historical.commit),
        receipt: result.receipt,
    })
}

pub(super) fn submit(
    service: &Service<'_>,
    request: SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<Value, String> {
    request.validate().map_err(|e| e.to_string())?;
    let correlation = derive_correlation(service.identity.clone(), &request.signed_payload)?;
    if correlation != request.correlation {
        return Err("submit signed correlation mismatch".into());
    }
    let pinned = service.pin(None)?;
    let existing = outcome(service, &pinned, &correlation)?;
    if matches!(existing, IntentOutcome::Unknown) {
        service.check_payload(&pinned, &request.signed_payload)?;
    } else {
        service.check_replay_payload(&pinned, &request.signed_payload)?;
    }
    let status = if matches!(existing, IntentOutcome::Unknown) {
        if let WorldServicePayloadV1::AgentChat(chat) = &request.signed_payload {
            agent_chat::validate_fresh(&pinned.world, chat)?;
        }
        if matches!(&request.signed_payload, WorldServicePayloadV1::GameplayJson(bytes)
            if gameplay::consumed_collect_data_nonce(&pinned.world, bytes)?)
        {
            // Canonical nonce knowledge is not a correlated historical receipt.
            // Preserve uncertainty instead of queueing a misleading replay.
            IntentOutcome::HistoryUnavailable
        } else if request
            .deadline_unix_ms
            .is_some_and(|deadline| deadline < super::super::now_unix_ms().max(0) as u64)
        {
            IntentOutcome::Expired
        } else {
            let digest = correlation::key_digest(&correlation.key)?;
            let hash = blake3::hash(digest.as_bytes());
            let action_id = u64::from_be_bytes(hash.as_bytes()[..8].try_into().unwrap()).max(1);
            let bytes = correlation::encode_consensus_intent(&request)?;
            service
                .runtime
                .lock()
                .map_err(|_| "node runtime lock poisoned")?
                .submit_consensus_action_payload(action_id, bytes)
                .map_err(|e| format!("{e:?}"))?;
            IntentOutcome::Received {
                durability: AdmissionDurability::Volatile,
            }
        }
    } else {
        existing
    };
    service.signed(
        SUBMIT_PATH,
        &request,
        IntentResponse {
            formal_profile: None,
            execution_evidence_scope: service.guarded.map(|_| {
                oasis7::world_service::verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE.into()
            }),
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            correlation,
            outcome: status,
        },
    )
}

pub(super) fn lookup(service: &Service<'_>, request: AuthenticatedLookup) -> Result<Value, String> {
    request.request.validate().map_err(|e| e.to_string())?;
    let correlation = derive_correlation(service.identity.clone(), &request.original)?;
    if correlation.key != request.request.key {
        return Err("lookup proof does not bind key".into());
    }
    let pinned = service.pin(None)?;
    let existing = outcome(service, &pinned, &correlation)?;
    if matches!(existing, IntentOutcome::Unknown) {
        service.check_payload(&pinned, &request.original)?;
    } else {
        service.check_replay_payload(&pinned, &request.original)?;
    }
    service.signed(
        LOOKUP_PATH,
        &request,
        IntentResponse {
            formal_profile: None,
            execution_evidence_scope: service.guarded.map(|_| {
                oasis7::world_service::verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE.into()
            }),
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            outcome: existing,
            correlation,
        },
    )
}

fn version<T: Serialize>(
    pinned: &PinnedWorld,
    scope: &str,
    projection: &T,
) -> Result<ProjectionVersion, String> {
    Ok(ProjectionVersion {
        commit: pinned.commit.clone(),
        projection_revision: request_digest("projection", projection)?,
        visibility_scope: scope.into(),
    })
}
fn cursor(pinned: &PinnedWorld, scope: &str, sequence: u64) -> EventCursor {
    EventCursor {
        stream_id: format!("domain-events:{}", pinned.commit.world.world_id),
        scope_id: scope.into(),
        era: pinned.world.snapshot().event_id_era,
        sequence,
        commit: pinned.commit.clone(),
    }
}
fn scoped_events<'a>(pinned: &'a PinnedWorld, agent: Option<&str>) -> Vec<&'a WorldEvent> {
    pinned
        .world
        .journal()
        .events
        .iter()
        .filter(|event| match &event.body {
            WorldEventBody::Domain(domain) => {
                agent.is_some_and(|agent| domain.agent_id() == Some(agent))
            }
            _ => false,
        })
        .collect()
}

pub(super) fn view(
    service: &Service<'_>,
    request: SignedReadRequest<ReadWorldViewRequest>,
) -> Result<Value, String> {
    view_profile(service, request, VIEW_PATH, false)
}

/// Explicit JSON projection codec; authorization and pinning share the typed read.
pub(super) fn view_json(
    service: &Service<'_>,
    request: SignedReadRequest<ReadWorldViewRequest>,
) -> Result<Value, String> {
    view_profile(
        service,
        request,
        oasis7_client_api::world_service::VIEW_JSON_PATH,
        true,
    )
}

fn view_profile(
    service: &Service<'_>,
    request: SignedReadRequest<ReadWorldViewRequest>,
    path: &str,
    json_projection: bool,
) -> Result<Value, String> {
    verify_read_request(path, &request)?;
    request.request.validate().map_err(|e| e.to_string())?;
    if request.request.world != service.identity {
        return Err("view world mismatch".into());
    }
    let pinned = service.pin(request.request.fixed_commit.as_ref())?;
    if let Some(minimum) = &request.request.min_commit
        && !pinned
            .commit
            .satisfies_minimum(minimum)
            .map_err(|e| e.to_string())?
    {
        return Err("read not caught up".into());
    }
    let agent = service.scope(
        &pinned,
        &request.request.scope_id,
        &request.subject_public_key,
    )?;
    let projection = projection::WorldServiceProjection::from_committed_world(
        &pinned.world,
        agent.as_deref(),
        &request.subject_public_key,
    )?;
    let response = ReadWorldViewResponse {
        formal_profile: None,
        formal_proof: None,
        execution_evidence_scope: service.guarded.map(|_| {
            oasis7::world_service::verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE.into()
        }),
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        version: version(&pinned, &request.request.scope_id, &projection)?,
        logical_tick: pinned.world.state().time,
        continuation: cursor(
            &pinned,
            &request.request.scope_id,
            projection.events.last().map_or(0, |event| event.id),
        ),
        view: projection,
    };
    response
        .validate(&request.request)
        .map_err(|e| e.to_string())?;
    if json_projection {
        // Normalize before signing, never reinterpret a typed-CBOR signature as JSON.
        let response: ReadWorldViewResponse<Value> =
            serde_json::from_value(serde_json::to_value(response).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        service.signed(path, &request, response)
    } else {
        service.signed(path, &request, response)
    }
}

pub(super) fn changes(
    service: &Service<'_>,
    request: SignedReadRequest<ReadWorldChangesRequest>,
) -> Result<Value, String> {
    verify_read_request(CHANGES_PATH, &request)?;
    request.request.validate().map_err(|e| e.to_string())?;
    let pinned = service.pin(None)?;
    let agent = service.scope(
        &pinned,
        &request.request.cursor.scope_id,
        &request.subject_public_key,
    )?;
    let expected = cursor(
        &pinned,
        &request.request.cursor.scope_id,
        request.request.cursor.sequence,
    );
    expected
        .validate_continuation(&request.request.cursor)
        .map_err(|e| e.to_string())?;
    let first = pinned
        .world
        .journal()
        .events
        .first()
        .map_or(0, |event| event.id);
    if request.request.cursor.sequence < first.saturating_sub(1) {
        return Err("cursor expired by journal retention".into());
    }
    if request.request.cursor.sequence
        > pinned
            .world
            .journal()
            .events
            .last()
            .map_or(0, |event| event.id)
    {
        return Err("cursor beyond pinned history".into());
    }
    let mut response = ReadWorldChangesResponse {
        formal_profile: None,
        execution_evidence_scope: service.guarded.map(|_| {
            oasis7::world_service::verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE.into()
        }),
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        changes: Vec::new(),
        next_cursor: request.request.cursor.clone(),
    };
    for event in scoped_events(&pinned, agent.as_deref())
        .into_iter()
        .filter(|event| event.id > request.request.cursor.sequence)
    {
        if response.changes.len() >= request.request.max_items.min(1024) as usize {
            break;
        }
        let next = cursor(&pinned, &request.request.cursor.scope_id, event.id);
        let previous = response.next_cursor.clone();
        response.changes.push(WorldChange {
            cursor: next.clone(),
            change: serde_json::to_value(event).map_err(|e| e.to_string())?,
        });
        response.next_cursor = next;
        if serde_json::to_vec(&response)
            .map_err(|e| e.to_string())?
            .len() as u64
            > request.request.max_bytes.min(1024 * 1024)
        {
            response.changes.pop();
            response.next_cursor = previous;
            break;
        }
    }
    response
        .validate(&request.request)
        .map_err(|e| e.to_string())?;
    if serde_json::to_vec(&response)
        .map_err(|e| e.to_string())?
        .len() as u64
        > request.request.max_bytes
    {
        return Err("changes response byte bound too small".into());
    }
    service.signed(CHANGES_PATH, &request, response)
}

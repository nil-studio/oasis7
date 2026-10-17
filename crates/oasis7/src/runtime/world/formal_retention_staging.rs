//! Content-addressed retention staging and bounded candidate selection.
use super::*;

pub(super) fn formal_journal_prefix_to_archive(
    events: &[crate::runtime::WorldEvent],
) -> Result<usize, String> {
    let mut encoded_event_lengths = Vec::with_capacity(events.len());
    for event in events {
        encoded_event_lengths.push(
            oasis7_wasm_abi::encode_canonical_cbor(event)
                .map_err(|error| error.to_string())?
                .len(),
        );
    }

    let mut hot_count = 0usize;
    let mut hot_event_bytes = 0usize;
    let mut prefix_count = events.len();
    for (index, event_bytes) in encoded_event_lengths.iter().enumerate().rev() {
        if hot_count >= HOT_FORMAL_JOURNAL_EVENTS {
            break;
        }
        let Some(next_event_bytes) = hot_event_bytes.checked_add(*event_bytes) else {
            break;
        };
        let next_hot_count = hot_count + 1;
        let Some(next_total_bytes) =
            cbor_array_header_bytes(next_hot_count).checked_add(next_event_bytes)
        else {
            break;
        };
        if next_total_bytes > HOT_FORMAL_JOURNAL_CBOR_BYTES {
            break;
        }
        hot_count = next_hot_count;
        hot_event_bytes = next_event_bytes;
        prefix_count = index;
    }

    let expected_hot_bytes = cbor_array_header_bytes(hot_count)
        .checked_add(hot_event_bytes)
        .ok_or("formal journal hot-byte count overflow")?;
    let actual_hot_bytes = oasis7_wasm_abi::encode_canonical_cbor(&&events[prefix_count..])
        .map_err(|error| error.to_string())?
        .len();
    if actual_hot_bytes != expected_hot_bytes
        || actual_hot_bytes > HOT_FORMAL_JOURNAL_CBOR_BYTES
        || events.len().saturating_sub(prefix_count) > HOT_FORMAL_JOURNAL_EVENTS
    {
        return Err("formal journal canonical CBOR budget calculation mismatch".into());
    }
    Ok(prefix_count)
}

pub(super) fn cbor_array_header_bytes(length: usize) -> usize {
    if length <= 23 {
        1
    } else if length <= u8::MAX as usize {
        2
    } else if length <= u16::MAX as usize {
        3
    } else if length <= u32::MAX as usize {
        5
    } else {
        9
    }
}

pub(super) fn reserve_plan_bytes(used: &mut usize, additional: usize) -> bool {
    let Some(next) = used.checked_add(additional) else {
        return false;
    };
    if next > MAX_ESTIMATED_NEW_LEAF_BYTES_PER_PLAN {
        return false;
    }
    *used = next;
    true
}

pub(super) fn encoded_map_entry_bytes<K: Serialize, V: Serialize>(
    key: &K,
    value: &V,
) -> Result<usize, String> {
    serde_cbor::to_vec(&(key, value))
        .map(|bytes| bytes.len().saturating_add(1))
        .map_err(|error| error.to_string())
}

pub(super) fn hot_suffix_start<T>(
    candidates: &[T],
    max_count: usize,
    byte_budget: usize,
    fixed_bytes_without_header: usize,
    entry_bytes: impl Fn(&T) -> usize,
) -> usize {
    let mut retained = 0usize;
    let mut used = fixed_bytes_without_header.saturating_add(16);
    for candidate in candidates.iter().rev() {
        if retained == max_count {
            break;
        }
        let Some(next) = used.checked_add(entry_bytes(candidate)) else {
            break;
        };
        if next > byte_budget {
            break;
        }
        retained += 1;
        used = next;
    }
    candidates.len().saturating_sub(retained)
}

pub(super) fn capability_hot_bytes(
    world: &World,
    prune_candidates: &[CapabilityNonceCandidate],
) -> Result<usize, String> {
    let nonce_keys: BTreeSet<_> = prune_candidates
        .iter()
        .map(|candidate| candidate.nonce_key.clone())
        .collect();
    capability_hot_bytes_for_keys(world, &nonce_keys)
}

pub(super) fn capability_hot_bytes_for_keys(
    world: &World,
    nonce_keys: &BTreeSet<String>,
) -> Result<usize, String> {
    let mut receipt_ids: BTreeSet<_> = nonce_keys
        .iter()
        .filter_map(|nonce_key| world.capability_nonce_records.get(nonce_key))
        .filter_map(|record| record.committed_receipt_id.clone())
        .collect();
    receipt_ids.retain(|receipt_id| {
        !world
            .capability_nonce_records
            .iter()
            .any(|(nonce_key, record)| {
                !nonce_keys.contains(nonce_key)
                    && record.committed_receipt_id.as_deref() == Some(receipt_id.as_str())
            })
    });

    let mut context_keys = BTreeSet::new();
    for nonce_key in nonce_keys {
        let Some(record) = world.capability_nonce_records.get(nonce_key) else {
            return Err("capability nonce disappeared during sizing".into());
        };
        let Some(receipt_id) = record.committed_receipt_id.as_deref() else {
            continue;
        };
        let Some(receipt) = world.capability_authorization_receipts.get(receipt_id) else {
            continue;
        };
        for stored in retained_invocation_contexts_for_receipt(world, receipt)? {
            if !invocation_context_is_referenced_by_unpruned_nonce(
                world,
                nonce_keys,
                &stored.context,
            )? {
                context_keys.insert(stored.map_key);
            }
        }
    }

    let nonce_bytes = encoded_map_bytes(&world.capability_nonce_records, |key, _| {
        !nonce_keys.contains(key)
    })?;
    let receipt_bytes = encoded_map_bytes(&world.capability_authorization_receipts, |key, _| {
        !receipt_ids.contains(key)
    })?;
    let context_bytes = encoded_map_bytes(&world.capability_invocation_contexts, |key, _| {
        !context_keys.contains(key)
    })?;
    nonce_bytes
        .checked_add(receipt_bytes)
        .and_then(|used| used.checked_add(context_bytes))
        .ok_or_else(|| "capability hot byte count overflow".into())
}

pub(super) fn world_service_hot_bytes(
    world: &World,
    prune_keys: &[String],
) -> Result<usize, String> {
    let prune_keys: BTreeSet<_> = prune_keys.iter().map(String::as_str).collect();
    encoded_map_bytes(
        &world.capability_revocation_state.world_service_results,
        |key, _| !prune_keys.contains(key.as_str()),
    )
}

pub(super) fn encoded_map_bytes<K, V>(
    map: &BTreeMap<K, V>,
    mut keep_entry: impl FnMut(&K, &V) -> bool,
) -> Result<usize, String>
where
    K: Ord + Serialize,
    V: Serialize,
{
    let filtered: BTreeMap<&K, &V> = map
        .iter()
        .filter(|(key, value)| keep_entry(key, value))
        .collect();
    serde_cbor::to_vec(&filtered)
        .map(|bytes| bytes.len())
        .map_err(|error| error.to_string())
}

pub(super) fn world_service_candidates(
    world: &World,
) -> Result<Vec<WorldServiceCandidate>, String> {
    let mut candidates = Vec::new();
    for (map_key, value) in &world.capability_revocation_state.world_service_results {
        crate::runtime::service_admission::correlation::validate_result(map_key, value)?;
        let result: crate::runtime::service_admission::CanonicalIntentResultV1 =
            serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        result
            .request
            .validate()
            .map_err(|error| error.to_string())?;
        let key_digest = crate::runtime::service_admission::correlation::key_digest(
            &result.request.correlation.key,
        )?;
        if key_digest != *map_key {
            return Err("WorldService result map key/correlation mismatch".into());
        }
        let canonical_result_cbor = serde_cbor::to_vec(&result).map_err(|e| e.to_string())?;
        if canonical_result_cbor.len() > MAX_RESULT_BYTES_IN_LEAF {
            return Err("WorldService result exceeds retained-history leaf bound".into());
        }
        let hot_entry_bytes = encoded_map_entry_bytes(map_key, value)?;
        candidates.push(WorldServiceCandidate {
            map_key: map_key.clone(),
            key_digest,
            estimated_bytes: canonical_result_cbor.len().saturating_add(16 * 1024),
            hot_entry_bytes,
            result,
            canonical_result_cbor,
        });
    }
    candidates.sort_by(|left, right| {
        (
            left.result.committed_height,
            left.result.action_id,
            &left.map_key,
        )
            .cmp(&(
                right.result.committed_height,
                right.result.action_id,
                &right.map_key,
            ))
    });
    Ok(candidates)
}

pub(super) fn stage_world_service_result(
    world: &World,
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: &mut BTreeMap<String, RetainedHistoryLeafV1>,
    candidate: &WorldServiceCandidate,
) -> Result<(), String> {
    let value = RetainedWorldServiceResultV1 {
        correlation_key_digest: candidate.key_digest.clone(),
        canonical_result_digest: blake3::hash(&candidate.canonical_result_cbor)
            .to_hex()
            .to_string(),
        canonical_result_cbor: candidate.canonical_result_cbor.clone(),
    };
    stage_immutable_leaf(
        parent,
        reader,
        staged,
        "world-service-result",
        &candidate.key_digest,
        RetainedHistoryLeafV1::WorldServiceResult(value),
    )?;
    stage_owner_chat_nonce(parent, reader, staged, candidate)?;
    stage_agent_chat_intent(world, parent, reader, staged, candidate)?;
    stage_feedback_publication(parent, reader, staged, candidate)?;
    stage_game_task_control(world, parent, reader, staged, candidate)?;
    Ok(())
}

pub(super) fn stage_owner_chat_nonce(
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: &mut BTreeMap<String, RetainedHistoryLeafV1>,
    candidate: &WorldServiceCandidate,
) -> Result<(), String> {
    let crate::runtime::service_admission::WorldServicePayloadV1::AgentChat(request) =
        &candidate.result.request.signed_payload
    else {
        return Ok(());
    };
    let Some(auth) = request.auth.as_ref() else {
        return Ok(());
    };
    if auth.public_key.trim().is_empty() {
        return Ok(());
    }
    let owner_public_key_digest = digest_text("owner-public-key", &auth.public_key)?;
    let lookup_key = retained_key_digest("owner-chat-nonce-high-water", &owner_public_key_digest)
        .map_err(|error| error.to_string())?;
    let mut entry = match staged_or_parent(parent, reader, staged, &lookup_key)? {
        None => RetainedOwnerChatNonceHighWaterV1 {
            owner_public_key_digest: owner_public_key_digest.clone(),
            high_water_nonce: auth.nonce,
            witness_result_key_digest: candidate.key_digest.clone(),
        },
        Some(RetainedHistoryLeafV1::OwnerChatNonceHighWater(entry)) => {
            if entry.owner_public_key_digest != owner_public_key_digest
                || entry.high_water_nonce == 0
                || !is_correlation_key_digest(&entry.witness_result_key_digest)
            {
                return Err("retained owner nonce index record is malformed".into());
            }
            let witness = staged_or_parent_world_service_result(
                parent,
                reader,
                staged,
                &entry.witness_result_key_digest,
            )?
            .ok_or("retained owner nonce index witness is absent")?;
            let witness_nonce = match &witness.request.signed_payload {
                crate::runtime::service_admission::WorldServicePayloadV1::AgentChat(request) => {
                    if let Some(auth) = request.auth.as_ref()
                        && digest_text("owner-public-key", &auth.public_key)?
                            == owner_public_key_digest
                    {
                        Some(auth.nonce)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if witness_nonce != Some(entry.high_water_nonce) {
                return Err("retained owner nonce index witness mismatch".into());
            }
            entry
        }
        Some(_) => return Err("retained owner nonce index leaf kind mismatch".into()),
    };
    if entry.owner_public_key_digest != owner_public_key_digest {
        return Err("retained owner nonce index key mismatch".into());
    }
    if auth.nonce > entry.high_water_nonce
        || (auth.nonce == entry.high_water_nonce
            && candidate.key_digest < entry.witness_result_key_digest)
    {
        entry.high_water_nonce = auth.nonce;
        entry.witness_result_key_digest = candidate.key_digest.clone();
    }
    staged.insert(
        lookup_key,
        RetainedHistoryLeafV1::OwnerChatNonceHighWater(entry),
    );
    Ok(())
}

pub(super) fn stage_agent_chat_intent(
    world: &World,
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: &mut BTreeMap<String, RetainedHistoryLeafV1>,
    candidate: &WorldServiceCandidate,
) -> Result<(), String> {
    if candidate.result.rejected.is_some() {
        return Ok(());
    }
    let Some(intent_id) = candidate
        .result
        .receipt
        .get("intent_id")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(());
    };
    let crate::runtime::service_admission::WorldServicePayloadV1::AgentChat(request) =
        &candidate.result.request.signed_payload
    else {
        return Ok(());
    };
    if crate::runtime::service_admission::agent_chat::validate_owner(world, request).is_err() {
        return Ok(());
    }
    let Some(auth) = request.auth.as_ref() else {
        return Ok(());
    };
    let Some(authority) = request.canonical_authority.as_ref() else {
        return Ok(());
    };
    let (Some(world_id), Some(reorg_epoch), Some(authority_scope)) = (
        request.world_id.as_ref(),
        request.reorg_epoch,
        request.authority_scope.as_ref(),
    ) else {
        return Ok(());
    };
    let lookup = AgentChatIntentLookupV1 {
        agent_id: request.agent_id.clone(),
        intent_id: intent_id.to_owned(),
        owner_public_key_digest: digest_text("owner-public-key", &auth.public_key)?,
        player_id: auth.player_id.clone(),
        world_id: world_id.clone(),
        branch_id: authority.branch_id.clone(),
        identity_generation: authority.agent_identity_generation,
        reorg_epoch,
        authority_scope: authority_scope.clone(),
    };
    stage_bounded_refs(
        parent,
        reader,
        staged,
        "agent-chat-intent",
        &lookup,
        candidate.key_digest.clone(),
        RetainedIndexKind::AgentChatIntent,
        |result| chat_result_matches(result, &lookup).map_err(|error| error.to_string()),
    )
}

pub(super) fn stage_feedback_publication(
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: &mut BTreeMap<String, RetainedHistoryLeafV1>,
    candidate: &WorldServiceCandidate,
) -> Result<(), String> {
    if candidate.result.rejected.is_some() {
        return Ok(());
    }
    let crate::runtime::service_admission::WorldServicePayloadV1::Cognition(signed) =
        &candidate.result.request.signed_payload
    else {
        return Ok(());
    };
    let Some(lineage) = candidate.result.receipt.get("lineage") else {
        return Ok(());
    };
    let Some(feedback_id) = lineage
        .get("feedback_id")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(());
    };
    let Some(runtime_receipt_id) = lineage
        .get("receipt_id")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(());
    };
    let request = &signed.request.request;
    let lookup = FeedbackPublicationLookupV1 {
        agent_id: request.agent_id.clone(),
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        request_digest: request.request_digest.clone(),
        feedback_id: feedback_id.to_owned(),
        runtime_receipt_id: Some(runtime_receipt_id.to_owned()),
    };
    stage_bounded_refs(
        parent,
        reader,
        staged,
        "feedback-publication",
        &lookup,
        candidate.key_digest.clone(),
        RetainedIndexKind::FeedbackPublication,
        |result| Ok(feedback_result_matches(result, &lookup)),
    )
}

pub(super) fn stage_game_task_control(
    _world: &World,
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: &mut BTreeMap<String, RetainedHistoryLeafV1>,
    candidate: &WorldServiceCandidate,
) -> Result<(), String> {
    if candidate.result.rejected.is_some() {
        return Ok(());
    }
    let Some(lookup) =
        crate::runtime::formal_retained_history::game_task_result_lookup(&candidate.result)?
    else {
        return Ok(());
    };
    use crate::runtime::formal_retained_history::{RetainedTaskChainLinkV1, RetainedTaskChainV1};
    let index_key =
        retained_key_digest("game-task-control-chain-v1", &lookup).map_err(|e| e.to_string())?;
    let previous = match staged_or_parent(parent, reader, staged, &index_key)? {
        None => None,
        Some(RetainedHistoryLeafV1::GameTaskControlChainIndex(entry)) => Some(entry),
        Some(_) => return Err("retained task chain head type mismatch".into()),
    };
    let count = previous.as_ref().map_or(Ok(1), |entry| {
        entry
            .count
            .checked_add(1)
            .ok_or("retained task chain count overflow")
    })?;
    stage_immutable_leaf(
        parent,
        reader,
        staged,
        "game-task-control-link",
        &candidate.key_digest,
        RetainedHistoryLeafV1::GameTaskControlLink(RetainedTaskChainLinkV1 {
            result_key_digest: candidate.key_digest.clone(),
            previous: previous.map(|entry| entry.head),
        }),
    )?;
    staged.insert(
        index_key,
        RetainedHistoryLeafV1::GameTaskControlChainIndex(RetainedTaskChainV1 {
            head: candidate.key_digest.clone(),
            count,
        }),
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum RetainedIndexKind {
    AgentChatIntent,
    FeedbackPublication,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keep authenticated archive parent, reader, staged writes, index identity and exact result validator bound explicitly."
)]
fn stage_bounded_refs<T: Serialize>(
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: &mut BTreeMap<String, RetainedHistoryLeafV1>,
    namespace: &str,
    lookup: &T,
    result_key_digest: String,
    kind: RetainedIndexKind,
    matches_result: impl Fn(
        &crate::runtime::service_admission::CanonicalIntentResultV1,
    ) -> Result<bool, String>,
) -> Result<(), String> {
    let index_key = retained_key_digest(namespace, lookup).map_err(|error| error.to_string())?;
    let mut refs = match staged_or_parent(parent, reader, staged, &index_key)? {
        None => Vec::new(),
        Some(RetainedHistoryLeafV1::AgentChatIntentIndex(entry))
            if matches!(kind, RetainedIndexKind::AgentChatIntent) =>
        {
            entry.result_key_digests
        }
        Some(RetainedHistoryLeafV1::FeedbackPublicationIndex(entry))
            if matches!(kind, RetainedIndexKind::FeedbackPublication) =>
        {
            entry.result_key_digests
        }
        Some(_) => return Err("retained secondary index key/type mismatch".into()),
    };
    let reference_limit = match kind {
        RetainedIndexKind::FeedbackPublication => 1,
        RetainedIndexKind::AgentChatIntent => MAX_INDEXED_RESULT_REFS,
    };
    if refs.len() > reference_limit
        || refs.windows(2).any(|pair| pair[0] >= pair[1])
        || refs
            .iter()
            .any(|reference| !is_correlation_key_digest(reference))
    {
        return Err("retained secondary index references malformed".into());
    }
    for reference in &refs {
        let previous = staged_or_parent_world_service_result(parent, reader, staged, reference)?
            .ok_or("retained secondary index references an absent result")?;
        if !matches_result(&previous)? {
            return Err("retained secondary index references a nonmatching result".into());
        }
    }
    push_bounded_ref(&mut refs, result_key_digest, reference_limit);
    let leaf = match kind {
        RetainedIndexKind::AgentChatIntent => {
            RetainedHistoryLeafV1::AgentChatIntentIndex(RetainedResultRefsV1 {
                result_key_digests: refs,
            })
        }
        RetainedIndexKind::FeedbackPublication => {
            RetainedHistoryLeafV1::FeedbackPublicationIndex(RetainedResultRefsV1 {
                result_key_digests: refs,
            })
        }
    };
    staged.insert(index_key, leaf);
    Ok(())
}

pub(super) fn staged_or_parent_world_service_result(
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: &BTreeMap<String, RetainedHistoryLeafV1>,
    result_key_digest: &str,
) -> Result<Option<crate::runtime::service_admission::CanonicalIntentResultV1>, String> {
    let leaf_key = retained_key_digest("world-service-result", &result_key_digest)
        .map_err(|error| error.to_string())?;
    let Some(leaf) = staged_or_parent(parent, reader, staged, &leaf_key)? else {
        return Ok(None);
    };
    let RetainedHistoryLeafV1::WorldServiceResult(entry) = leaf else {
        return Err("retained WorldService result key/type mismatch".into());
    };
    if entry.correlation_key_digest != result_key_digest
        || !is_digest(&entry.canonical_result_digest)
    {
        return Err("retained WorldService result key/digest mismatch".into());
    }
    let result: crate::runtime::service_admission::CanonicalIntentResultV1 =
        serde_cbor::from_slice(&entry.canonical_result_cbor).map_err(|error| error.to_string())?;
    let canonical = serde_cbor::to_vec(&result).map_err(|error| error.to_string())?;
    if canonical != entry.canonical_result_cbor
        || blake3::hash(&canonical).to_hex().to_string() != entry.canonical_result_digest
        || crate::runtime::service_admission::correlation::key_digest(
            &result.request.correlation.key,
        )? != result_key_digest
    {
        return Err("retained WorldService result bytes/correlation mismatch".into());
    }
    let as_json = serde_json::to_value(&result).map_err(|error| error.to_string())?;
    crate::runtime::service_admission::correlation::validate_result(result_key_digest, &as_json)?;
    Ok(Some(result))
}

pub(super) fn push_bounded_ref(refs: &mut Vec<String>, value: String, limit: usize) {
    if refs.contains(&value) {
        return;
    }
    refs.push(value);
    refs.sort();
    refs.dedup();
    refs.truncate(limit);
}

pub(super) fn stage_immutable_leaf(
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: &mut BTreeMap<String, RetainedHistoryLeafV1>,
    namespace: &str,
    source_key: &str,
    value: RetainedHistoryLeafV1,
) -> Result<(), String> {
    let key_digest =
        retained_key_digest(namespace, &source_key).map_err(|error| error.to_string())?;
    stage_immutable_leaf_at_key(parent, reader, staged, key_digest, namespace, value)
}

pub(super) fn stage_immutable_leaf_at_key(
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: &mut BTreeMap<String, RetainedHistoryLeafV1>,
    key_digest: String,
    namespace: &str,
    value: RetainedHistoryLeafV1,
) -> Result<(), String> {
    if let Some(existing) = staged_or_parent(parent, reader, staged, &key_digest)?
        && existing != value
    {
        return Err(format!("immutable retained {namespace} record changed"));
    }
    staged.insert(key_digest, value);
    Ok(())
}

pub(super) fn staged_or_parent(
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: &BTreeMap<String, RetainedHistoryLeafV1>,
    key_digest: &str,
) -> Result<Option<RetainedHistoryLeafV1>, String> {
    if let Some(value) = staged.get(key_digest) {
        return Ok(Some(value.clone()));
    }
    parent
        .read_leaf(key_digest, reader)
        .map_err(|error| error.to_string())
}

pub(super) fn capability_nonce_candidates(
    world: &World,
) -> Result<Vec<CapabilityNonceCandidate>, String> {
    let mut candidates = Vec::new();
    for (nonce_key, record) in &world.capability_nonce_records {
        if record.state != "committed" {
            continue;
        }
        let Some(receipt_id) = record.committed_receipt_id.as_deref() else {
            continue;
        };
        let Some(receipt) = world.capability_authorization_receipts.get(receipt_id) else {
            continue;
        };
        if !capability_nonce_is_closed(world, nonce_key, record, receipt) {
            continue;
        }
        let invocation_contexts = retained_invocation_contexts_for_receipt(world, receipt)?;
        let invocation_context_keys = invocation_contexts
            .iter()
            .map(|stored| stored.map_key.clone())
            .collect();
        let value = RetainedCapabilityNonceV1 {
            nonce_key: nonce_key.clone(),
            record: record.clone(),
            receipt: receipt.clone(),
            invocation_contexts,
        };
        value.validate()?;
        let encoded = encode_retained_leaf(&RetainedHistoryLeafV1::CapabilityNonce(value.clone()))?;
        if encoded.len() > BUSINESS_INDEX_MAX_LEAF_BYTES {
            continue;
        }
        candidates.push(CapabilityNonceCandidate {
            nonce_key: nonce_key.clone(),
            invocation_context_keys,
            sort_key: (
                receipt.finality_epoch,
                receipt
                    .world_head_after
                    .unwrap_or(receipt.world_head_before),
                nonce_key.clone(),
            ),
            estimated_bytes: encoded.len().saturating_add(4096),
            value,
        });
    }
    candidates.sort_by(|left, right| left.sort_key.cmp(&right.sort_key));
    Ok(candidates)
}

pub(super) fn retained_invocation_contexts_for_receipt(
    world: &World,
    receipt: &CapabilityAuthorizationAuditReceipt,
) -> Result<Vec<RetainedCapabilityInvocationContextV1>, String> {
    let (Some(grant_id), Some(response_nonce)) = (
        receipt.grant_id.as_deref(),
        receipt.response_nonce.as_deref(),
    ) else {
        return Ok(Vec::new());
    };
    let canonical_key = capability_invocation_context_key(grant_id, response_nonce)?;
    let mut retained = Vec::new();
    for (map_key, context) in &world.capability_invocation_contexts {
        if context.grant_id != grant_id || context.response_nonce != response_nonce {
            continue;
        }
        if map_key != &canonical_key && map_key != grant_id {
            return Err("capability invocation context has a noncanonical map key".into());
        }
        if !crate::runtime::formal_retained_history::invocation_context_matches_receipt(
            context, receipt,
        )? {
            return Err("capability invocation context does not match its receipt".into());
        }
        retained.push(RetainedCapabilityInvocationContextV1 {
            map_key: map_key.clone(),
            context: context.clone(),
        });
    }
    retained.sort_by(|left, right| left.map_key.cmp(&right.map_key));
    Ok(retained)
}

pub(super) fn invocation_context_is_referenced_by_unpruned_nonce(
    world: &World,
    pruned_nonce_keys: &BTreeSet<String>,
    context: &crate::runtime::CapabilityInvocationContext,
) -> Result<bool, String> {
    for receipt in world
        .capability_authorization_receipts
        .values()
        .filter(|receipt| {
            receipt.grant_id.as_deref() == Some(context.grant_id.as_str())
                && receipt.response_nonce.as_deref() == Some(context.response_nonce.as_str())
        })
    {
        let nonce_ref = world.capability_nonce_records.iter().find(|(_, record)| {
            record.committed_receipt_id.as_deref() == Some(receipt.receipt_id.as_str())
        });
        let Some((nonce_key, _)) = nonce_ref else {
            // A context with an unpaired receipt cannot be proven terminal and
            // remains hot until a later candidate can account for it.
            return Ok(true);
        };
        if !pruned_nonce_keys.contains(nonce_key) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn capability_nonce_is_closed(
    world: &World,
    nonce_key: &str,
    record: &CapabilityAuthorizationNonceRecord,
    receipt: &CapabilityAuthorizationAuditReceipt,
) -> bool {
    let Some(receipt_id) = record.committed_receipt_id.as_deref() else {
        return false;
    };
    if receipt.receipt_id != receipt_id
        || !matches!(receipt.decision.as_str(), "accepted" | "denied")
        || receipt
            .world_head_after
            .is_none_or(|head| head <= receipt.world_head_before)
        || receipt.authorization_nonce_key_hash.as_deref() != Some(nonce_key)
        || record.request_hash != receipt.canonical_request_hash
        || record.outcome_hash != receipt.canonical_result_hash
        || world
            .capability_effect_receipt_links
            .values()
            .any(|link| link.authorization_receipt_id == receipt_id)
    {
        return false;
    }
    let committed_effect_ids: BTreeSet<&str> = receipt
        .committed_effect_receipt_ids
        .iter()
        .map(String::as_str)
        .chain(
            receipt
                .committed_effect_receipt_id
                .iter()
                .map(String::as_str),
        )
        .collect();
    if world
        .pending_effects
        .iter()
        .any(|effect| committed_effect_ids.contains(effect.identity_key().as_str()))
        || world
            .inflight_effects
            .keys()
            .any(|intent_id| committed_effect_ids.contains(intent_id.as_str()))
    {
        return false;
    }
    // Do not accept malformed key identities as a terminal archive candidate;
    // leaving them hot preserves the existing fail-closed replay path.
    is_digest(nonce_key)
}

pub(super) fn agent_intent_candidates(world: &World) -> Result<Vec<AgentIntentCandidate>, String> {
    let mut candidates = Vec::new();
    for (intent_id, intent) in &world.state.agent_intent_ledger {
        if !is_terminal_agent_intent(&intent.status) {
            continue;
        }
        if intent.intent_id != *intent_id {
            return Err("AgentIntent map key differs from its durable intent id".into());
        }
        if world.state.agents.values().any(|agent| {
            agent
                .intent
                .as_ref()
                .is_some_and(|current| current.intent_id == *intent_id)
        }) {
            continue;
        }
        if intent.effect_intent_id.as_ref().is_some_and(|effect_id| {
            world
                .pending_effects
                .iter()
                .any(|effect| effect.intent_id == *effect_id || effect.identity_key() == *effect_id)
                || world.inflight_effects.values().any(|effect| {
                    effect.intent_id == *effect_id || effect.identity_key() == *effect_id
                })
                || world
                    .capability_effect_receipt_links
                    .contains_key(effect_id)
        }) {
            continue;
        }
        let value = RetainedAgentIntentV1::new(intent.clone())?;
        let encoded = encode_retained_leaf(&RetainedHistoryLeafV1::AgentIntent(value.clone()))?;
        if encoded.len() > BUSINESS_INDEX_MAX_LEAF_BYTES {
            continue;
        }
        candidates.push(AgentIntentCandidate {
            intent_id: intent_id.clone(),
            sort_key: (intent.updated_at, intent.event_seq, intent_id.clone()),
            estimated_bytes: encoded.len().saturating_add(2048),
            hot_entry_bytes: encoded_map_entry_bytes(intent_id, intent)?,
            value,
        });
    }
    candidates.sort_by(|left, right| left.sort_key.cmp(&right.sort_key));
    Ok(candidates)
}

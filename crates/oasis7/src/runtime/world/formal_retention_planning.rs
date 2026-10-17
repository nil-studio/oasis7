//! Bounded retention planning over immutable committed material.
use super::*;

/// Build a bounded retention transition against a parent index whose root was
/// extracted from an exact snapshot hash checked by a qualified profile-v2
/// anchor. `world` may be the post-execution candidate; its full state hash is
/// therefore intentionally not compared with the parent hash. Its inherited
/// retention descriptor must remain byte-for-byte the parent descriptor.
pub(crate) fn prepare_world_retention_candidate(
    world: &World,
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
) -> Result<FormalWorldRetentionPlanV1, String> {
    parent.root().validate()?;
    let candidate_snapshot = serde_cbor::to_vec(&world.snapshot()).map_err(|e| e.to_string())?;
    parent.verify_inherited_descriptor(&candidate_snapshot)?;

    let mut staged = BTreeMap::<String, RetainedHistoryLeafV1>::new();
    let mut estimated_new_leaf_bytes = 0usize;
    let tick_consensus_archive = world.prepare_formal_tick_consensus_archive(parent, reader)?;
    for leaf in &tick_consensus_archive.immutable_leaves {
        let value: RetainedHistoryLeafV1 =
            serde_cbor::from_slice(&leaf.canonical_value).map_err(|error| error.to_string())?;
        if encode_retained_leaf(&value)? != leaf.canonical_value {
            return Err("Tick archive leaf is not canonical".into());
        }
        if !reserve_plan_bytes(
            &mut estimated_new_leaf_bytes,
            leaf.canonical_value.len().saturating_add(1024),
        ) {
            return Err("Tick archive exceeds retained-history plan byte bound".into());
        }
        stage_immutable_leaf_at_key(
            parent,
            reader,
            &mut staged,
            leaf.key_digest.clone(),
            "world-tick-consensus",
            value,
        )?;
    }

    let mut wsr_candidates = world_service_candidates(world)?;
    let wsr_cut = hot_suffix_start(
        &wsr_candidates,
        HOT_WORLD_SERVICE_RESULTS,
        HOT_WORLD_SERVICE_RESULT_BYTES,
        0,
        |candidate| candidate.hot_entry_bytes,
    );
    let wsr_cut = wsr_cut.min(MAX_ARCHIVES_PER_CLASS_PER_PLAN);
    let mut prune_wsr = Vec::new();
    for candidate in wsr_candidates.drain(..wsr_cut) {
        if prune_wsr.len() >= MAX_ARCHIVES_PER_CLASS_PER_PLAN
            || !reserve_plan_bytes(&mut estimated_new_leaf_bytes, candidate.estimated_bytes)
        {
            break;
        }
        stage_world_service_result(world, parent, reader, &mut staged, &candidate)?;
        prune_wsr.push(candidate.map_key);
    }
    if world_service_hot_bytes(world, &prune_wsr)? > HOT_WORLD_SERVICE_RESULT_BYTES
        || world
            .capability_revocation_state
            .world_service_results
            .len()
            .saturating_sub(prune_wsr.len())
            > HOT_WORLD_SERVICE_RESULTS
    {
        return Err(
            "WorldService history cannot fit the hot count/byte budget within this archive plan"
                .into(),
        );
    }

    let mut nonce_candidates = capability_nonce_candidates(world)?;
    let mut nonce_cut = nonce_candidates
        .len()
        .saturating_sub(HOT_COMMITTED_CAPABILITY_NONCES);
    let all_nonce_candidates_fit =
        capability_hot_bytes(world, &nonce_candidates)? <= HOT_CAPABILITY_CLOSURE_BYTES;
    if !all_nonce_candidates_fit {
        return Err("non-archivable capability authorization state exceeds hot byte budget".into());
    }
    while nonce_cut < nonce_candidates.len()
        && capability_hot_bytes(world, &nonce_candidates[..nonce_cut])?
            > HOT_CAPABILITY_CLOSURE_BYTES
    {
        nonce_cut += 1;
    }
    let nonce_cut = nonce_cut.min(MAX_ARCHIVES_PER_CLASS_PER_PLAN);
    let mut prune_nonces = Vec::new();
    let mut pruned_receipts = BTreeSet::new();
    let mut archived_invocation_contexts = Vec::new();
    for candidate in nonce_candidates.drain(..nonce_cut) {
        if prune_nonces.len() >= MAX_ARCHIVES_PER_CLASS_PER_PLAN
            || !reserve_plan_bytes(&mut estimated_new_leaf_bytes, candidate.estimated_bytes)
        {
            break;
        }
        stage_immutable_leaf(
            parent,
            reader,
            &mut staged,
            "capability-nonce",
            &candidate.nonce_key,
            RetainedHistoryLeafV1::CapabilityNonce(candidate.value.clone()),
        )?;
        if let Some(receipt_id) = &candidate.value.record.committed_receipt_id {
            pruned_receipts.insert(receipt_id.clone());
        }
        archived_invocation_contexts.extend(
            candidate
                .invocation_context_keys
                .into_iter()
                .map(|map_key| (map_key, candidate.value.clone())),
        );
        prune_nonces.push(candidate.nonce_key);
    }
    let pruned_nonce_set: BTreeSet<_> = prune_nonces.iter().cloned().collect();
    let remaining_committed_nonce_count = world
        .capability_nonce_records
        .iter()
        .filter(|(nonce_key, record)| {
            record.state == "committed" && !pruned_nonce_set.contains(nonce_key.as_str())
        })
        .count();
    if capability_hot_bytes_for_keys(world, &pruned_nonce_set)? > HOT_CAPABILITY_CLOSURE_BYTES
        || remaining_committed_nonce_count > HOT_COMMITTED_CAPABILITY_NONCES
    {
        return Err(
            "capability history cannot fit the hot count/byte budget within this archive plan"
                .into(),
        );
    }

    let nonce_prune_set: BTreeSet<_> = prune_nonces.iter().cloned().collect();
    let mut prune_invocation_contexts = BTreeSet::new();
    for (map_key, retained_nonce) in archived_invocation_contexts {
        let Some(context) = retained_nonce
            .invocation_contexts
            .iter()
            .find(|stored| stored.map_key == map_key)
            .map(|stored| &stored.context)
        else {
            return Err("retained invocation context disappeared during planning".into());
        };
        if !invocation_context_is_referenced_by_unpruned_nonce(world, &nonce_prune_set, context)? {
            prune_invocation_contexts.insert(map_key);
        }
    }

    let mut intent_candidates = agent_intent_candidates(world)?;
    let intent_ids: BTreeSet<_> = intent_candidates
        .iter()
        .map(|candidate| candidate.intent_id.as_str())
        .collect();
    let fixed_intent_bytes =
        encoded_map_bytes(&world.state.agent_intent_ledger, |intent_id, _| {
            !intent_ids.contains(intent_id.as_str())
        })?;
    if fixed_intent_bytes > HOT_AGENT_INTENT_BYTES {
        return Err("nonterminal or linked AgentIntent state exceeds hot byte budget".into());
    }
    let intent_cut = hot_suffix_start(
        &intent_candidates,
        HOT_TERMINAL_AGENT_INTENTS,
        HOT_AGENT_INTENT_BYTES,
        fixed_intent_bytes.saturating_sub(16),
        |candidate| candidate.hot_entry_bytes,
    )
    .min(MAX_ARCHIVES_PER_CLASS_PER_PLAN);
    let mut prune_intents = Vec::new();
    for candidate in intent_candidates.drain(..intent_cut) {
        if prune_intents.len() >= MAX_ARCHIVES_PER_CLASS_PER_PLAN
            || !reserve_plan_bytes(&mut estimated_new_leaf_bytes, candidate.estimated_bytes)
        {
            break;
        }
        stage_immutable_leaf(
            parent,
            reader,
            &mut staged,
            "agent-intent",
            &candidate.intent_id,
            RetainedHistoryLeafV1::AgentIntent(candidate.value),
        )?;
        prune_intents.push(candidate.intent_id);
    }
    let pruned_intent_set: BTreeSet<_> = prune_intents.iter().map(String::as_str).collect();
    let remaining_intent_count = world
        .state
        .agent_intent_ledger
        .iter()
        .filter(|(intent_id, intent)| {
            is_terminal_agent_intent(&intent.status)
                && !pruned_intent_set.contains(intent_id.as_str())
        })
        .count();
    let remaining_intent_bytes =
        encoded_map_bytes(&world.state.agent_intent_ledger, |intent_id, _| {
            !pruned_intent_set.contains(intent_id.as_str())
        })?;
    if remaining_intent_count > HOT_TERMINAL_AGENT_INTENTS
        || remaining_intent_bytes > HOT_AGENT_INTENT_BYTES
    {
        return Err(
            "AgentIntent history cannot fit the hot count/byte budget within this archive plan"
                .into(),
        );
    }

    // This compact summary preserves the cognition runtime's historical
    // queries: zero/one/multiple cardinality, the sole epoch when cardinality
    // is one, and the maximum epoch used by the `world.reorg_epoch` precondition.
    let epoch_key =
        retained_key_digest("agent-intent-reorg-epochs", &()).map_err(|error| error.to_string())?;
    let prior_epochs = match parent
        .read_leaf(&epoch_key, reader)
        .map_err(|error| error.to_string())?
    {
        None => DistinctReorgEpochSummaryV1::Empty,
        Some(RetainedHistoryLeafV1::AgentIntentReorgEpochs(summary)) => summary,
        Some(_) => return Err("retained AgentIntent epoch summary kind mismatch".into()),
    };
    let candidate_epochs = DistinctReorgEpochSummaryV1::merge_hot(
        &prior_epochs,
        world
            .state
            .agent_intent_ledger
            .values()
            .map(|intent| &intent.reorg_epoch),
    );
    if candidate_epochs != prior_epochs {
        if !reserve_plan_bytes(&mut estimated_new_leaf_bytes, 1024) {
            return Err("retained-history plan byte bound".into());
        }
        staged.insert(
            epoch_key,
            RetainedHistoryLeafV1::AgentIntentReorgEpochs(candidate_epochs),
        );
    }

    let journal_cursors = world.formal_journal_event_cursors()?;
    let journal_prefix_count = formal_journal_prefix_to_archive(&world.journal.events)?;
    if journal_prefix_count > MAX_JOURNAL_EVENTS_TO_ARCHIVE_PER_PLAN {
        return Err("formal journal archive prefix exceeds the per-plan event bound".into());
    }
    let journal_events_to_prune = journal_cursors[..journal_prefix_count].to_vec();
    for (cursor, event) in journal_events_to_prune
        .iter()
        .zip(world.journal.events.iter())
    {
        let retained = RetainedJournalEventV1::encode(cursor.0, event)?;
        if retained.event_cbor.len() > MAX_RESULT_BYTES_IN_LEAF {
            return Err("formal journal event exceeds retained-history leaf bound".into());
        }
        let value = RetainedHistoryLeafV1::JournalEvent(retained);
        let encoded_leaf = encode_retained_leaf(&value)?;
        if encoded_leaf.len() > BUSINESS_INDEX_MAX_LEAF_BYTES {
            return Err("formal journal event leaf exceeds business-index bound".into());
        }
        if !reserve_plan_bytes(
            &mut estimated_new_leaf_bytes,
            encoded_leaf.len().saturating_add(1024),
        ) {
            return Err("formal journal archive exceeds retained-history plan byte bound".into());
        }
        let key_digest = retained_key_digest("world-journal-event", cursor)
            .map_err(|error| error.to_string())?;
        stage_immutable_leaf_at_key(
            parent,
            reader,
            &mut staged,
            key_digest,
            "world-journal-event",
            value,
        )?;
        let (locator_key, locator) =
            crate::runtime::formal_retained_history::retained_tick_journal_leaf(cursor.0, event)?;
        let locator_bytes = encode_retained_leaf(&locator)?;
        if !reserve_plan_bytes(
            &mut estimated_new_leaf_bytes,
            locator_bytes.len().saturating_add(1024),
        ) {
            return Err("formal journal locator exceeds retained-history plan byte bound".into());
        }
        stage_immutable_leaf_at_key(
            parent,
            reader,
            &mut staged,
            locator_key,
            "world-journal-tick-event",
            locator,
        )?;
    }
    let remaining_journal = &world.journal.events[journal_prefix_count..];
    if remaining_journal.len() > HOT_FORMAL_JOURNAL_EVENTS
        || oasis7_wasm_abi::encode_canonical_cbor(&remaining_journal)
            .map_err(|error| error.to_string())?
            .len()
            > HOT_FORMAL_JOURNAL_CBOR_BYTES
    {
        return Err("formal journal cannot fit its hot count/byte budget".into());
    }

    // A receipt can be removed from the hot map only when every hot nonce
    // reference to it is also archived. Keep shared references in the hot map.
    pruned_receipts.retain(|receipt_id| {
        let still_hot_reference = world.capability_nonce_records.iter().any(|(key, record)| {
            !nonce_prune_set.contains(key)
                && record.committed_receipt_id.as_deref() == Some(receipt_id.as_str())
        });
        !still_hot_reference
    });

    let updates = materialize_changed_updates(parent, reader, staged)?;
    if updates.len() > BUSINESS_INDEX_MAX_UPDATES {
        return Err("retained-history index update limit exceeded".into());
    }
    let (proposed_root, delta) = if updates.is_empty() {
        (parent.root().clone(), None)
    } else {
        let delta =
            BusinessIndexDeltaV1::build_v2(parent.root().index_root.clone(), updates, reader)?;
        let descriptor = FormalRetainedHistoryRootV1::from_index_root(delta.new.clone())?;
        (descriptor, Some(delta))
    };
    Ok(FormalWorldRetentionPlanV1 {
        base_root: parent.root().clone(),
        proposed_root,
        delta,
        world_service_result_keys_to_prune: prune_wsr,
        capability_nonce_keys_to_prune: prune_nonces,
        capability_receipt_ids_to_prune: pruned_receipts.into_iter().collect(),
        capability_invocation_context_keys_to_prune: prune_invocation_contexts
            .into_iter()
            .collect(),
        agent_intent_ids_to_prune: prune_intents,
        journal_events_to_prune,
        tick_consensus_archive,
    })
}

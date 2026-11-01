use super::*;

impl ProviderOriginalMaterialOwnerV1 {
    pub(super) fn pending_protection_root_impl(&self) -> Result<[u8; 32], String> {
        let inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let _workspace = self
            .memory
            .reserve(
                3 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES,
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        pending_root_digest(&self.owner, &inner.head)
    }

    pub(super) fn protected_roots_page_impl(
        &self,
        cursor: Option<&oasis7::simulator::ProviderPendingRootsCursorV1>,
        limit: usize,
    ) -> Result<oasis7::simulator::ProviderPendingRootsPageV1, String> {
        if limit == 0 || limit > MAX_PROTECTED_ROOT_PAGE {
            return Err("provider protected-root page bound".into());
        }
        let inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let _page_workspace = self
            .memory
            .reserve(64 * 1024, MemoryClass::Recovery)
            .map_err(|error| error.to_string())?;
        let root_digest = pending_root_digest(&self.owner, &inner.head)?;
        let cursor_state = match cursor {
            Some(cursor) => {
                let state = cursor
                    .owner_state::<ProtectedRootsCursorState>()
                    .ok_or("provider protected-root cursor owner/type")?;
                if state.pending_root != root_digest {
                    return Err("provider protected-root cursor root changed".into());
                }
                if state.after_key_digest.is_none()
                    || state.active_attempts_seen > inner.head.pending.leaf_count
                {
                    return Err("provider protected-root cursor state".into());
                }
                *state
            }
            None => ProtectedRootsCursorState {
                pending_root: root_digest,
                after_key_digest: None,
                active_attempts_seen: 0,
            },
        };

        // Admit every retained page carrier before collecting indexed leaves.
        // The Merkle path cursor avoids re-scanning prior leaves on each page.
        let _row_charge = self
            .memory
            .reserve(
                limit.saturating_mul(
                    std::mem::size_of::<(String, [u8; 32])>()
                        .saturating_add(MAX_OPERATION_ID_BYTES),
                ),
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        let object_charge = self
            .memory
            .reserve(
                limit
                    .saturating_mul(6)
                    .saturating_mul(std::mem::size_of::<[u8; 32]>()),
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        let attempt_charge = self
            .memory
            .reserve(
                limit.saturating_mul(
                    std::mem::size_of::<ProviderPendingAttemptSummaryV1>()
                        .saturating_add(MAX_OWNER_ID_BYTES)
                        .saturating_add(MAX_OPERATION_ID_BYTES),
                ),
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        let roots_charge = self
            .memory
            .reserve(
                3usize.saturating_mul(
                    std::mem::size_of::<oasis7_formal_protocol::incremental_state::StateRootV1>()
                        + 128,
                ),
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        let owner_charge = self
            .memory
            .reserve(
                std::mem::size_of::<ProviderMaterialOwnerBindingV1>() + MAX_OWNER_ID_BYTES,
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        let mut selected = Vec::<(String, [u8; 32])>::new();
        selected
            .try_reserve_exact(limit)
            .map_err(|_| "provider protected-root page allocation")?;
        if selected.capacity() > limit {
            return Err("provider protected-root page allocation exceeded admission".into());
        }
        let mut physical = finite_page_io();
        let session = self
            .pages
            .state_session(physical.clone(), MemoryClass::Recovery)
            .map_err(|error| error.to_string())?;
        let mut scan_budget = oasis7_formal_protocol::incremental_state::StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 256 * 1024 * 1024,
            leaves_remaining: limit as u64,
        };
        let prefix = b"oasis7.iws.provider-pending-operation-key.v1\0";
        let page = {
            let mut scan = oasis7_formal_protocol::incremental_state::StateScanContext {
                source: &session,
                memory: &self.memory,
                class: MemoryClass::Recovery,
                io: &mut scan_budget,
            };
            scan.visit_entries_page(
                &inner.head.pending,
                cursor_state.after_key_digest,
                limit,
                |entry| {
                    if !entry.key.starts_with(prefix) {
                        return Err("provider pending key domain".to_owned());
                    }
                    let operation = std::str::from_utf8(&entry.key[prefix.len()..])
                        .map_err(|_| "provider pending operation UTF-8".to_owned())?;
                    operation_key(operation).map_err(|error| error.to_string())?;
                    let mut operation_owned = String::new();
                    operation_owned
                        .try_reserve_exact(operation.len())
                        .map_err(|_| "provider protected-root operation allocation".to_owned())?;
                    if operation_owned.capacity() > MAX_OPERATION_ID_BYTES {
                        return Err(
                            "provider protected-root operation allocation exceeded admission"
                                .into(),
                        );
                    }
                    operation_owned.push_str(operation);
                    selected.push((operation_owned, entry.value));
                    Ok(())
                },
                || self.cancelled(),
            )
            .map_err(|error| error.to_string())
        };
        if let Some(error) = session.take_read_error() {
            return Err(error.to_string());
        }
        let page = page?;
        if page.visited as usize != selected.len() {
            return Err("provider protected-root page count mismatch".into());
        }
        physical = session.remaining_io().map_err(|error| error.to_string())?;

        let active_attempts_seen = cursor_state
            .active_attempts_seen
            .checked_add(page.visited)
            .ok_or("provider pending page count overflow")?;
        if !page.has_more && active_attempts_seen != inner.head.pending.leaf_count {
            return Err("provider pending pages do not cover the root leaf count".into());
        }
        let next_cursor = if page.has_more {
            let next_after = page
                .last_key_digest
                .ok_or("provider pending page lacks continuation path")?;
            let cursor_charge = self
                .memory
                .reserve(
                    std::mem::size_of::<oasis7::simulator::ProviderPendingRootsCursorV1>()
                        + std::mem::size_of::<ProtectedRootsCursorState>(),
                    MemoryClass::Recovery,
                )
                .map_err(|error| error.to_string())?;
            let cursor = oasis7::simulator::ProviderPendingRootsCursorV1::from_owner_state(
                ProtectedRootsCursorState {
                    pending_root: root_digest,
                    after_key_digest: Some(next_after),
                    active_attempts_seen,
                },
            );
            Some(Arc::new(cursor_charge.retain(cursor)))
        } else {
            None
        };
        let mut object_ids = Vec::new();
        let max_object_ids = limit.saturating_mul(6);
        object_ids
            .try_reserve_exact(max_object_ids)
            .map_err(|_| "provider protected-root page allocation")?;
        if object_ids.capacity() > max_object_ids {
            return Err("provider protected-root page allocation exceeded admission".into());
        }
        let mut attempts = Vec::new();
        attempts
            .try_reserve_exact(limit)
            .map_err(|_| "provider protected-attempt page allocation")?;
        if attempts.capacity() > limit {
            return Err("provider protected-attempt page allocation exceeded admission".into());
        }
        for (operation_id, event_id) in selected {
            let record = self.read_event_object(&event_id, MemoryClass::Recovery, &mut physical)?;
            if !record.phase.is_pending()
                || record.operation_id != operation_id
                || record.sequence > inner.head.sequence
            {
                return Err("provider pending index/event identity mismatch".into());
            }
            let event_index = self.read_state_value(
                &inner.head.events,
                &event_key(record.sequence),
                MemoryClass::Recovery,
                &mut physical,
            )?;
            let seen_index = self.read_state_value(
                &inner.head.seen,
                &operation_key(&operation_id)?,
                MemoryClass::Recovery,
                &mut physical,
            )?;
            if event_index != Some(event_id) || seen_index != Some(event_id) {
                return Err("provider pending event is not linked by owner roots".into());
            }
            let attempt = self.attempt_from_event(&record);
            let phase = match record.phase {
                EventPhase::RequestPrepared => ProviderPendingAttemptPhaseV1::RequestPrepared,
                EventPhase::DispatchedOrUnknown => {
                    ProviderPendingAttemptPhaseV1::DispatchedOrUnknown
                }
                EventPhase::ResponseBodyPlanned => {
                    ProviderPendingAttemptPhaseV1::ResponseBodyPlanned
                }
                EventPhase::ResponseBodyDurable => {
                    ProviderPendingAttemptPhaseV1::ResponseBodyDurable
                }
                EventPhase::ResponseContextPlanned => {
                    ProviderPendingAttemptPhaseV1::ResponseContextPlanned
                }
                EventPhase::ResponseCaptured => ProviderPendingAttemptPhaseV1::ResponseCaptured,
                EventPhase::RejectionEvidencePlanned => {
                    ProviderPendingAttemptPhaseV1::RejectionEvidencePlanned
                }
                EventPhase::Committed | EventPhase::Rejected => {
                    return Err("provider pending index references terminal event".into());
                }
            };
            attempts.push(ProviderPendingAttemptSummaryV1 {
                attempt,
                phase,
                response_object_id: record.response_object_id,
                response_context_object_id: record.response_context_object_id,
                response_status: record.response_status,
                terminal_evidence_object_id: record.terminal_evidence_object_id,
            });
            // The event record itself holds the durable phase and references
            // for this pending attempt, so it must remain protected alongside
            // the request/response objects it names.
            object_ids.push(event_id);
            object_ids.push(record.request_object_id);
            object_ids.push(record.request_context_object_id);
            if let Some(response) = record.response_object_id {
                object_ids.push(response);
            }
            if let Some(context) = record.response_context_object_id {
                object_ids.push(context);
            }
            if let Some(evidence) = record.terminal_evidence_object_id {
                object_ids.push(evidence);
            }
        }
        object_ids.sort_unstable();
        object_ids.dedup();
        Ok(oasis7::simulator::ProviderPendingRootsPageV1 {
            owner: Arc::new(owner_charge.retain(self.owner.clone())),
            pending_root: root_digest,
            object_ids: Arc::new(object_charge.retain(object_ids)),
            attempts: Arc::new(attempt_charge.retain(attempts)),
            state_roots: Arc::new(roots_charge.retain(ProviderPendingStateRootsV1 {
                events: inner.head.events.clone(),
                pending: inner.head.pending.clone(),
                seen: inner.head.seen.clone(),
            })),
            next_cursor,
        })
    }
}

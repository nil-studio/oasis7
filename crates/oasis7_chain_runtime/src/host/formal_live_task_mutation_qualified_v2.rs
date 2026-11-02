//! Same-submit bounded observation; no lookup, resend, Current or remote I/O.
use super::*;
use oasis7_agent_api::game::DecimalU64;
use oasis7_client_api::world_service::{SignedServiceResponse, task_mutation_qualified_v2::*};
struct QualifiedAdmission {
    ack: TaskMutationAdmissionV2,
    submitter: String,
}
fn canonical(q: &TaskMutationSubmittedV2) -> Result<(Vec<u8>, String, u64), String> {
    match q {
        TaskMutationSubmittedV2::GuardedStop(inner) => Ok((
            correlation::encode_guarded_stop_consensus_v2(inner)?,
            authority::request_digest(q.kind().path(), inner)?,
            inner.original.request.execute_until_ms.0,
        )),
        TaskMutationSubmittedV2::RevokeAndClose(inner) => Ok((
            correlation::encode_cleanup_consensus_v2(inner)?,
            authority::request_digest(q.kind().path(), inner)?,
            inner.original.request.cleanup_until_ms.0,
        )),
    }
}
pub(super) fn handle(
    service: &FormalLiveService,
    q: TaskMutationWaitSubmitV2,
    expected: TaskMutationKindV2,
    body: &[u8],
) -> HttpReply {
    let started = Instant::now();
    let admitted = (|| -> Result<_, String> {
        q.validate()?;
        if q.submitted.kind() != expected {
            return Err("qualified mutation path/kind differs".into());
        }
        let received = checked_now(service)?;
        if received >= q.delivery_until_ms.0 {
            return Err("qualified mutation wait already expired".into());
        }
        let (raw, bare_digest, execute_until) = canonical(&q.submitted)?;
        let ack = admit_bounded(service, &q, expected, &raw, execute_until, started)?;
        Ok((raw, bare_digest, ack, received))
    })();
    let (raw, bare_digest, ack, received) = match admitted {
        Ok(admitted) => admitted,
        Err(error) => {
            return raw_error(
                expected.path(),
                body,
                "mutation_not_queued",
                &error,
                &service.signer,
            );
        }
    };
    let actual_submitter = ack.submitter;
    let ack = ack.ack;
    let admission = match authority::sign_service_response(
        expected.path(),
        bare_digest,
        ack,
        &hex::encode(service.signer.to_bytes()),
    ) {
        Ok(admission) => admission,
        Err(_) => {
            return raw_error(
                expected.path(),
                body,
                "outcome_unknown",
                "mutation queued; admission reply unavailable",
                &service.signer,
            );
        }
    };
    // Admission already happened. A later clock/lock/evidence/signing failure
    // retains the original signed ACK instead of claiming it never queued.
    let retained_ack = admission.clone();
    #[cfg(test)]
    diagnostic::begin(service, &q);
    let waited = wait(
        service,
        &q,
        &raw,
        admission,
        received,
        started,
        &actual_submitter,
    );
    #[cfg(test)]
    if let Err(error) = &waited {
        diagnostic::failure(service, &q, error);
    }
    let reply = waited.unwrap_or(TaskMutationSubmitReplyV2::Queued {
        admission: retained_ack,
        wait_outcome: TaskMutationWaitOutcomeV2::Unavailable,
    });
    json_reply(200, &reply)
}

fn wait(
    service: &FormalLiveService,
    q: &TaskMutationWaitSubmitV2,
    raw: &[u8],
    admission: SignedServiceResponse<TaskMutationAdmissionV2>,
    received: u64,
    started: Instant,
    actual_submitter: &str,
) -> Result<TaskMutationSubmitReplyV2, String> {
    let until = q.delivery_until_ms.0.min(
        received
            .checked_add(5_000)
            .ok_or("qualified mutation wait clock overflow")?,
    );
    let mut inspected = None;
    loop {
        let sampled = checked_now(service);
        #[cfg(test)]
        if let Err(error) = &sampled {
            diagnostic::failure(service, q, error);
        }
        #[cfg(test)]
        let absolute_expired = sampled.as_ref().is_ok_and(|now| *now >= until);
        let now = match sampled {
            Ok(now) if now < until && started.elapsed() < HTTP_CONNECTION_TIMEOUT => now,
            _ => {
                #[cfg(test)]
                diagnostic::note(
                    service,
                    q,
                    if absolute_expired {
                        diagnostic::Reason::PollAbsoluteExpired
                    } else {
                        diagnostic::Reason::PollConnectionTimeout
                    },
                );
                return Ok(TaskMutationSubmitReplyV2::Queued {
                    admission,
                    wait_outcome: TaskMutationWaitOutcomeV2::Expired,
                });
            }
        };
        if !ready(service)? {
            #[cfg(test)]
            diagnostic::note(service, q, diagnostic::Reason::NotReady);
            return Ok(TaskMutationSubmitReplyV2::Queued {
                admission,
                wait_outcome: TaskMutationWaitOutcomeV2::Unavailable,
            });
        }
        // Clone only opaque metadata/Arc handles under the publication lock.
        // Decode original bytes and validate resident results after unlocking.
        let candidate = service
            .published
            .current
            .try_lock()
            .map_err(|_| "qualified mutation publication busy or poisoned")?
            .as_ref()
            .map(|cut| {
                (
                    cut.original_material.clone(),
                    cut.stop_publication.clone(),
                    cut.anchor.head().head().record_id.clone(),
                )
            });
        let (material, publication, observed_record_id) = match candidate {
            Some((Some(material), Some(publication), record_id)) => {
                (material, publication, record_id)
            }
            _ => {
                #[cfg(test)]
                diagnostic::note(service, q, diagnostic::Reason::MissingMaterial);
                return Ok(TaskMutationSubmitReplyV2::Queued {
                    admission,
                    wait_outcome: TaskMutationWaitOutcomeV2::Unavailable,
                });
            }
        };
        if observed_record_id != material.after().head().head().record_id {
            #[cfg(test)]
            diagnostic::note(service, q, diagnostic::Reason::MaterialHeadMismatch);
            return Ok(TaskMutationSubmitReplyV2::Queued {
                admission,
                wait_outcome: TaskMutationWaitOutcomeV2::Unavailable,
            });
        }
        {
            let record_id = material.after().head().head().record_id.clone();
            if inspected.as_ref() != Some(&record_id) {
                inspected = Some(record_id);
                if let Ok(context) =
                    material.exact_singleton_execution(raw, admission.payload.action_id.0)
                {
                    let [action] = context.committed_actions.as_slice() else {
                        return Err("qualified mutation exact singleton absent".into());
                    };
                    if action.submitter_player_id != actual_submitter {
                        return Err(
                            "qualified mutation differs from actual configured submitter".into(),
                        );
                    }
                    let catalog_digest = action.payload_hash.clone();
                    let committed_at = u64::try_from(context.committed_at_unix_ms)
                        .map_err(|_| "qualified mutation clock invalid")?;
                    if committed_at < admission.payload.admitted_at_ms.0 || committed_at > now {
                        #[cfg(test)]
                        diagnostic::note(service, q, diagnostic::Reason::CommitClockMismatch);
                        return Ok(TaskMutationSubmitReplyV2::Queued {
                            admission,
                            wait_outcome: TaskMutationWaitOutcomeV2::Unavailable,
                        });
                    }
                    let execution = TaskMutationExecutionV2 {
                        height: DecimalU64(context.height),
                        slot: DecimalU64(context.slot),
                        epoch: DecimalU64(context.epoch),
                        node_block_hash: context.node_block_hash,
                        action_root: context.action_root,
                        committed_at_unix_ms: DecimalU64(committed_at),
                    };
                    let extracted = {
                        let publication = publication
                            .try_lock()
                            .map_err(|_| "qualified mutation resident cut busy or poisoned")?;
                        oasis7::world_service::task_mutation_effect_projection_v2::exact_result(
                            &publication,
                            &q.submitted,
                            &execution,
                        )
                    };
                    if let Ok((commit, result)) = extracted {
                        if !ready(service)? || !same_current(service, material.after())? {
                            #[cfg(test)]
                            diagnostic::note(service, q, diagnostic::Reason::FreshHeadUnavailable);
                            return Ok(TaskMutationSubmitReplyV2::Queued {
                                admission,
                                wait_outcome: TaskMutationWaitOutcomeV2::Unavailable,
                            });
                        }
                        let delivered = checked_now(service)?;
                        if delivered >= until || started.elapsed() >= HTTP_CONNECTION_TIMEOUT {
                            #[cfg(test)]
                            diagnostic::note(service, q, diagnostic::Reason::BeforeSigningExpired);
                            return Ok(TaskMutationSubmitReplyV2::Queued {
                                admission,
                                wait_outcome: TaskMutationWaitOutcomeV2::Expired,
                            });
                        }
                        let payload = TaskMutationQualifiedEffectV2 {
                            api_revision: 2,
                            kind: q.submitted.kind(),
                            path: q.submitted.kind().path().into(),
                            world: q.submitted.world().clone(),
                            chain_id: material.after().head().record().world.chain_id.clone(),
                            delivery_nonce: q.delivery_nonce.clone(),
                            original_digest: q.submitted.original_digest()?,
                            action_id: admission.payload.action_id,
                            canonical_catalog_digest: catalog_digest,
                            execution,
                            witnessed_in_commit: commit,
                            result,
                        };
                        let domain = match q.submitted.kind() {
                            TaskMutationKindV2::GuardedStop => STOP_QUALIFIED_EFFECT_DOMAIN_V2,
                            TaskMutationKindV2::RevokeAndClose => CLOSE_QUALIFIED_EFFECT_DOMAIN_V2,
                        };
                        let effect = authority::sign_service_response(
                            domain,
                            q.digest()?,
                            payload,
                            &hex::encode(service.signer.to_bytes()),
                        )?;
                        let reply = TaskMutationSubmitReplyV2::Qualified {
                            admission,
                            effect: Box::new(effect),
                        };
                        if serde_json::to_vec(&reply)
                            .map_err(|error| error.to_string())?
                            .len()
                            > 64 * 1024
                        {
                            let TaskMutationSubmitReplyV2::Qualified { admission, .. } = reply
                            else {
                                unreachable!()
                            };
                            #[cfg(test)]
                            diagnostic::note(service, q, diagnostic::Reason::ReplyBound);
                            return Ok(TaskMutationSubmitReplyV2::Queued {
                                admission,
                                wait_outcome: TaskMutationWaitOutcomeV2::Unavailable,
                            });
                        }
                        // Includes actual proof parsing, Node signature and full reply encoding.
                        let delivered = checked_now(service)?;
                        let expired =
                            delivered >= until || started.elapsed() >= HTTP_CONNECTION_TIMEOUT;
                        if expired || !ready(service)? || !same_current(service, material.after())?
                        {
                            let TaskMutationSubmitReplyV2::Qualified { admission, .. } = reply
                            else {
                                unreachable!()
                            };
                            #[cfg(test)]
                            diagnostic::note(
                                service,
                                q,
                                if expired {
                                    diagnostic::Reason::FinalDeliveryExpired
                                } else {
                                    diagnostic::Reason::FinalDeliveryUnavailable
                                },
                            );
                            return Ok(TaskMutationSubmitReplyV2::Queued {
                                admission,
                                wait_outcome: if expired {
                                    TaskMutationWaitOutcomeV2::Expired
                                } else {
                                    TaskMutationWaitOutcomeV2::Unavailable
                                },
                            });
                        }
                        return Ok(reply);
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn checked_now(service: &FormalLiveService) -> Result<u64, String> {
    service
        .stop_record_clock
        .try_lock()
        .map_err(|_| "qualified mutation clock busy or poisoned")?
        .sample()
}
fn try_admit_once(
    service: &FormalLiveService,
    q: &TaskMutationWaitSubmitV2,
    kind: TaskMutationKindV2,
    raw: &[u8],
    until: u64,
) -> Result<Option<QualifiedAdmission>, String> {
    let Some(_gate) = available(&service.admission_gate)? else {
        return Ok(None);
    };
    let Some(runtime) = available(&service.runtime)? else {
        return Ok(None);
    };
    let (node_running, node_error) = runtime
        .try_closed_task_mutation_status()
        .map_err(|error| format!("Node status unavailable: {error:?}"))?;
    // Same ready predicate as runtime_status; bounded reads never wait for a
    // hook, publication or status lock and do not grant task admission by flag.
    let cut_present = {
        let Some(cut) = available(&service.published.current)? else {
            return Ok(None);
        };
        cut.is_some()
    };
    let state_error = {
        let Some(error) = available(&service.state.last_error)? else {
            return Ok(None);
        };
        error.is_some()
    };
    if !service.state.ready.load(Ordering::Acquire)
        || service.state.stopping.load(Ordering::Acquire)
        || !node_running
        || !cut_present
        || state_error
        || node_error
        || q.submitted.world() != &service.world
    {
        return Err("qualified Node/world unavailable".into());
    }
    let now = checked_now(service)?;
    if now > until || (kind == TaskMutationKindV2::GuardedStop && now == until) {
        return Err("task mutation expired before queue admission".into());
    }
    let original_digest = q.submitted.original_digest()?;
    let id = mutation_action_id_v2(kind, q.submitted.world(), &original_digest)?;
    // Existing closed path still performs all real signature/target/preflight
    // checks; its hook acquisition is already try_lock and BFT-unsupported.
    let submitter = runtime.config().player_id.clone();
    runtime
        .try_submit_closed_task_mutation_payload(id, raw.to_vec())
        .map_err(|error| format!("task mutation queue rejected: {error:?}"))?;
    Ok(Some(QualifiedAdmission {
        submitter,
        ack: TaskMutationAdmissionV2 {
            api_revision: 2,
            kind,
            world: q.submitted.world().clone(),
            original_digest,
            action_id: DecimalU64(id),
            admitted_at_ms: DecimalU64(now),
            outcome: TaskMutationQueuedOutcomeV2::QueuedVolatile,
        },
    }))
}

fn available<T>(lock: &Mutex<T>) -> Result<Option<std::sync::MutexGuard<'_, T>>, String> {
    match lock.try_lock() {
        Ok(guard) => Ok(Some(guard)),
        Err(std::sync::TryLockError::WouldBlock) => Ok(None),
        Err(std::sync::TryLockError::Poisoned(_)) => {
            Err("qualified mutation local lock poisoned".into())
        }
    }
}
fn admit_bounded(
    service: &FormalLiveService,
    q: &TaskMutationWaitSubmitV2,
    kind: TaskMutationKindV2,
    raw: &[u8],
    until: u64,
    started: Instant,
) -> Result<QualifiedAdmission, String> {
    loop {
        if started.elapsed() >= HTTP_CONNECTION_TIMEOUT
            || checked_now(service)? >= q.delivery_until_ms.0
        {
            return Err("qualified mutation admission window ended; not queued".into());
        }
        if let Some(ack) = try_admit_once(service, q, kind, raw, until)? {
            return Ok(ack);
        }
        // Only retry local lock acquisition: no submission or I/O has occurred.
        // Once actual submit is attempted its result is returned without retry.
        thread::sleep(Duration::from_millis(2));
    }
}

// All waits use bounded try-lock observations. Busy/poisoned state is unknown,
// never a reason to retain a shared gate or fabricate a fresh qualified cut.
fn ready(service: &FormalLiveService) -> Result<bool, String> {
    if !service.state.ready.load(Ordering::Acquire)
        || service.state.stopping.load(Ordering::Acquire)
    {
        return Ok(false);
    }
    let Some(runtime) = available(&service.runtime)? else {
        return Err("qualified mutation runtime busy".into());
    };
    let (running, error) = runtime
        .try_closed_task_mutation_status()
        .map_err(|e| format!("Node status unavailable: {e:?}"))?;
    drop(runtime);
    let Some(cut) = available(&service.published.current)? else {
        return Err("qualified mutation publication busy".into());
    };
    let present = cut.is_some();
    drop(cut);
    let Some(state_error) = available(&service.state.last_error)? else {
        return Err("qualified mutation status busy".into());
    };
    Ok(running && !error && present && state_error.is_none())
}
fn same_current(
    service: &FormalLiveService,
    expected: &VerifiedFormalReadAnchorV1,
) -> Result<bool, String> {
    let Some(cut) = available(&service.published.current)? else {
        return Err("qualified mutation delivery busy".into());
    };
    Ok(cut
        .as_ref()
        .is_some_and(|cut| cut.anchor.head().head() == expected.head().head()))
}

#[cfg(test)]
mod diagnostic {
    use super::*;
    #[derive(Clone, Copy)]
    pub(super) enum Reason {
        PollAbsoluteExpired,
        PollConnectionTimeout,
        NotReady,
        MissingMaterial,
        MaterialHeadMismatch,
        CommitClockMismatch,
        FreshHeadUnavailable,
        BeforeSigningExpired,
        ReplyBound,
        FinalDeliveryExpired,
        FinalDeliveryUnavailable,
        ClockBusy,
        RuntimeBusy,
        PublicationBusy,
        ResidentBusy,
        StatusBusy,
        DeliveryBusy,
        OtherWaitError,
    }
    impl Reason {
        fn label(self) -> &'static str {
            match self {
                Self::PollAbsoluteExpired => "PollAbsoluteExpired",
                Self::PollConnectionTimeout => "PollConnectionTimeout",
                Self::NotReady => "NotReady",
                Self::MissingMaterial => "MissingMaterial",
                Self::MaterialHeadMismatch => "MaterialHeadMismatch",
                Self::CommitClockMismatch => "CommitClockMismatch",
                Self::FreshHeadUnavailable => "FreshHeadUnavailable",
                Self::BeforeSigningExpired => "BeforeSigningExpired",
                Self::ReplyBound => "ReplyBound",
                Self::FinalDeliveryExpired => "FinalDeliveryExpired",
                Self::FinalDeliveryUnavailable => "FinalDeliveryUnavailable",
                Self::ClockBusy => "ClockBusy",
                Self::RuntimeBusy => "RuntimeBusy",
                Self::PublicationBusy => "PublicationBusy",
                Self::ResidentBusy => "ResidentBusy",
                Self::StatusBusy => "StatusBusy",
                Self::DeliveryBusy => "DeliveryBusy",
                Self::OtherWaitError => "OtherWaitError",
            }
        }
    }
    type Key = (usize, String);
    static RECORDS: std::sync::OnceLock<Mutex<std::collections::BTreeMap<Key, Option<Reason>>>> =
        std::sync::OnceLock::new();
    fn records() -> &'static Mutex<std::collections::BTreeMap<Key, Option<Reason>>> {
        RECORDS.get_or_init(|| Mutex::new(std::collections::BTreeMap::new()))
    }
    fn key(service: &FormalLiveService, q: &TaskMutationWaitSubmitV2) -> Key {
        (
            service as *const FormalLiveService as usize,
            q.digest().unwrap(),
        )
    }
    pub(super) fn begin(service: &FormalLiveService, q: &TaskMutationWaitSubmitV2) {
        records().lock().unwrap().insert(key(service, q), None);
    }
    pub(super) fn note(service: &FormalLiveService, q: &TaskMutationWaitSubmitV2, reason: Reason) {
        if let Some(slot) = records().lock().unwrap().get_mut(&key(service, q)) {
            if slot.is_none() {
                *slot = Some(reason);
            }
        }
    }
    pub(super) fn failure(service: &FormalLiveService, q: &TaskMutationWaitSubmitV2, error: &str) {
        let reason = match error {
            "qualified mutation clock busy or poisoned" => Reason::ClockBusy,
            "qualified mutation runtime busy" => Reason::RuntimeBusy,
            "qualified mutation publication busy"
            | "qualified mutation publication busy or poisoned" => Reason::PublicationBusy,
            "qualified mutation resident cut busy or poisoned" => Reason::ResidentBusy,
            "qualified mutation status busy" => Reason::StatusBusy,
            "qualified mutation delivery busy" => Reason::DeliveryBusy,
            _ => Reason::OtherWaitError,
        };
        note(service, q, reason);
    }
    pub(super) fn take(
        service: &FormalLiveService,
        q: &TaskMutationWaitSubmitV2,
    ) -> Option<&'static str> {
        records()
            .lock()
            .unwrap()
            .remove(&key(service, q))
            .flatten()
            .map(Reason::label)
    }
}
#[cfg(test)]
pub(super) fn wait_diagnostic(
    service: &FormalLiveService,
    q: &TaskMutationWaitSubmitV2,
) -> Option<&'static str> {
    diagnostic::take(service, q)
}

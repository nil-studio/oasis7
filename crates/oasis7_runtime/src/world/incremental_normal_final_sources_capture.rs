use super::*;
use crate::world::incremental_factory_phase::io::{
    FactoryPhaseMeterLaneV1, FactoryPhaseMeterLanesV1,
};
use oasis7_world_state::{
    incremental_cognition_control_overlay::read_current_cognition_service_control_v2,
    incremental_cognition_fields::CognitionControlFieldV1 as CF,
    incremental_cognition_read::CognitionControlReadV1,
};
struct HashLoan<'a>(&'a mut CognitionHeadHashWorkV1);
impl FactoryPhaseMeterLanesV1 for HashLoan<'_> {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        v(FactoryPhaseMeterLaneV1::Steps(
            &mut self.0.hash_nodes_remaining,
        ));
        v(FactoryPhaseMeterLaneV1::Bytes(
            &mut self.0.input_bytes_remaining,
        ));
    }
}
struct Compare<'a> {
    expected: &'a [u8],
    at: usize,
    equal: bool,
}
impl std::io::Write for Compare<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        let end = self
            .at
            .checked_add(b.len())
            .ok_or_else(|| std::io::Error::other("Cognition head length"))?;
        if self.expected.get(self.at..end) != Some(b) {
            self.equal = false;
        }
        self.at = end;
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn overlay(
    e: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1,
) -> BodyMutationError {
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1 as E;
    match e {
        E::Execution(e) => e.into(),
        E::Value(e) => e.into(),
        E::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + StatePageWriter
        + oasis7_formal_protocol::incremental_schedule::SchedulePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader
        + crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
{
    fn check_final_cognition_head(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        fields: (CF, CF),
        head: &CognitionHistoryHeadV1,
        present: bool,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>> {
        let mut text = [0u8; 71];
        let expected = if let Some(d) = head.digest {
            text[..7].copy_from_slice(b"blake3:");
            hex::encode_to_slice(d, &mut text[7..]).map_err(|_| bad())?;
            &text[..]
        } else {
            &text[..0]
        };
        common.charge(2, 256).map_err(formal)?;
        with_current_root_module_meter_v1(self, common, live_owner, |root, live| {
            let guard = root.ctx.transaction.guard();
            let fence = live();
            let q = parent.refs().storage.current().cognition();
            let seq = read_current_cognition_service_control_v2(
                fields.0,
                q,
                parent.refs().roots,
                root.ctx,
                &mut std::io::sink(),
                0,
                &guard,
                &fence,
            )
            .map_err(overlay)?;
            if present {
                if seq != CognitionControlReadV1::Unsigned(head.sequence) {
                    return Err(bad().into());
                }
            } else if seq != CognitionControlReadV1::Absent {
                return Err(bad().into());
            }
            let guard = root.ctx.transaction.guard();
            let fence = live();
            let mut compare = Compare {
                expected,
                at: 0,
                equal: true,
            };
            let digest = read_current_cognition_service_control_v2(
                fields.1,
                q,
                parent.refs().roots,
                root.ctx,
                &mut compare,
                71,
                &guard,
                &fence,
            )
            .map_err(overlay)?;
            if present {
                if digest
                    != (CognitionControlReadV1::Text {
                        bytes: expected.len() as u64,
                    })
                    || !compare.equal
                    || compare.at != expected.len()
                {
                    return Err(bad().into());
                }
            } else if digest != CognitionControlReadV1::Absent {
                return Err(bad().into());
            }
            Ok(())
        })
    }
    pub(in crate::world) fn capture_normal_final_sources_v1<'p, P, PE, PC>(
        &mut self,
        tick: &mut NormalTickCurrentDataV1<'p>,
        parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        tail: &NormalTickTailSourcesV1<'_>,
        retention: super::super::incremental_normal_retention::NormalRetentionParticipantV1,
        bootstrap: Option<
            &Arc<
                ReservedValue<crate::world::normal_bootstrap_current::BootstrapEconomyFrontiersV1>,
            >,
        >,
        authorization:&super::super::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'_>,
        revocation: &oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationCurrentV1,
        business_parent:&ReservedValue<super::super::incremental_normal_business_commitment::NormalBusinessMetadataParentSourceV1>,
        projection_parent: &ReservedValue<
            super::super::incremental_normal_projection_commitment::AcceptedNormalProjectionCacheV1,
        >,
        parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        delivery:&mut super::super::incremental_normal_industry_due::delivery::CurrentProductValidationDeliveryIndexV1<'_>,
        work: &mut NormalFinalSourcesWorkV1,
        cut_work: &mut NormalHostFinishCutWorkV1,
        parent_caches: &ReservedValue<
            super::super::incremental_normal_parent_caches::NormalParentCachesV1,
        >,
        compiler: &crate::module_source_compiler::RuntimeSourceCompiler,
        process: &super::super::incremental_normal_process_source::NormalProcessSourceV1,
        actions: &mut super::super::incremental_normal_action_work::NormalActionWorkSetV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        Arc<ReservedValue<CapturedNormalFinalSourcesV1>>,
        RootModuleExecutionErrorV1<O::Error>,
    >
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        tick.validate(self.ctx.transaction, parent, input, common, &live_owner())?;
        let before_guard = self.ctx.transaction.guard();
        if !before_guard.same_cut(tick.final_source_guard()) {
            return Err(bad().into());
        }
        let before =
            capture_normal_write_fact_cut_v1(self.ctx.transaction, cut_work, common, live_owner)?;
        let history = parent.refs().runtime;
        let memory = common.memory().clone();
        let clone_index = |q: &ReservedValue<RuntimeHistoryIndexQualificationV1>,
                           common: &mut FormalBoundedContextV1<'_>,
                           cancel: &mut dyn FnMut() -> bool|
         -> Result<
            ReservedValue<RuntimeHistoryIndexQualificationV1>,
            BodyMutationError,
        > {
            common.charge(1, 1024).map_err(formal)?;
            Ok(select_runtime_history_window_v1(
                q,
                q.window().start,
                q.window().count,
                &memory,
                || cancel(),
            )?)
        };
        let snapshot = clone_index(
            history.history_index(RuntimeHistoryLaneV1::Snapshot),
            common,
            &mut self.ctx.cancelled,
        )?;
        let audit = clone_index(
            history.history_index(RuntimeHistoryLaneV1::Audit),
            common,
            &mut self.ctx.cancelled,
        )?;
        let ticks = clone_index(
            tick.consensus_for_final_source().index(),
            common,
            &mut self.ctx.cancelled,
        )?;
        let mut journal = clone_index(
            history.history_index(RuntimeHistoryLaneV1::Journal),
            common,
            &mut self.ctx.cancelled,
        )?;
        // Any codec/IO/admission error or unwind must poison this same TX;
        // the original typed error remains the caller-visible first cause.
        let event_attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let notes = self.gameplay_journal_notes_v1(
                history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()].count,
                common,
            )?;
            let mut event_builder = super::super::incremental_normal_event_commitment::NormalEventCommitmentBuilderV1::new(
            notes.len(), self.ctx.transaction.guard(), common,
        )?;
            for note in notes.iter().rev() {
                note.validate(self.ctx.transaction)?;
                let (decoded_event, bytes) = read_gameplay_world_event_original_v1(
                    note.original(),
                    None,
                    self.originals,
                    &mut work.originals,
                    common,
                    &mut self.ctx.cancelled,
                )
                .map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
                event_builder.push(decoded_event, common)?;
                let ordinal = u64::try_from(note.ordinal()).map_err(|_| bad())?;
                let location = FactLocationV1 {
                    height: parent.refs().roots.height.checked_add(1).ok_or_else(bad)?,
                    ordinal,
                    original: *note.original(),
                };
                journal = with_current_root_history_index_meter_v1(
                    self,
                    common,
                    live_owner,
                    &mut work.runtime_index,
                    |root, live, index_work| {
                        let mut read = WorldFieldReadContextV1 {
                            root: parent.refs().roots.state_root(StateNamespaceV1::Artifacts),
                            state: root.state,
                            values: ValueReadContext {
                                source: root.ctx.values.source,
                                memory: root.ctx.values.memory,
                                class: root.ctx.values.class,
                                io: &mut *root.ctx.values.io,
                            },
                            key_work: &mut *root.ctx.key_work,
                            semantic_bytes: root.ctx.semantic_bytes,
                            record_bytes: root.ctx.record_bytes,
                            take_state_error: &mut root.ctx.take_state_error,
                            cancelled: &mut root.ctx.cancelled,
                        };
                        let next = prepare_runtime_history_index_append_v1(
                            &journal,
                            RuntimeHistoryIndexRowV1 {
                                location,
                                encoded_bytes: bytes,
                            },
                            &mut read,
                            root.members,
                            index_work,
                        )
                        .map_err(|e| {
                            RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(
                                e,
                            )))
                        })?;
                        root.ctx
                            .transaction
                            .validate_guard(&root.ctx.transaction.guard(), &live())?;
                        Ok(next)
                    },
                )?;
            }
            let event_commitment = event_builder.finish(common)?;
            event_commitment.validate_capture(
                self.ctx.transaction,
                &before_guard,
                common,
                &live_owner(),
            )?;
            Ok::<_, RootModuleExecutionErrorV1<O::Error>>(event_commitment)
        }));
        let event_commitment = match event_attempt {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                return Err(error);
            }
            Err(payload) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(payload)
            }
        };
        common.charge(1, 1024).map_err(formal)?;
        let count = journal
            .window()
            .total
            .min(tail.economic.retention.max_events() as u64);
        let start = journal.window().total.checked_sub(count).ok_or_else(bad)?;
        journal = select_runtime_history_window_v1(
            &journal,
            start,
            count,
            &memory,
            &mut self.ctx.cancelled,
        )?;
        let old = parent.refs().cognition.selected().manifest();
        let enabled = old.enabled;
        common.charge(8, 8192).map_err(formal)?;
        let mut metadata_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<CapturedCognitionViewV1>() + 4 * 512,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut heads = old.heads.clone();
        let mut indexes = old.indexes.clone();
        let economy_frontier = if let Some(source) = bootstrap {
            source.validate(self.ctx.transaction, common.memory(), &live_owner())?;
            if !source.uses_budget(common.memory()) || !enabled {
                return Err(bad().into());
            }
            common.charge(4, 2048).map_err(formal)?;
            let economy = source.journal();
            let provision = source.provision();
            heads[1] = CognitionHistoryHeadV1 {
                sequence: economy.sequence(),
                digest: economy.head_digest_v1(),
            };
            heads[2] = CognitionHistoryHeadV1 {
                sequence: provision.sequence(),
                digest: provision.head_digest_v1(),
            };
            indexes[2] = CognitionHistoryIndexRefV1 {
                present: true,
                records: economy.records(),
                root: economy.index_root().clone(),
            };
            indexes[3] = CognitionHistoryIndexRefV1 {
                present: true,
                records: provision.records(),
                root: provision.index_root().clone(),
            };
            Some(Arc::clone(source))
        } else {
            None
        };
        if enabled {
            let slot = tick.cognition_for_final_source();
            if slot.is_none() {
                let mut loan = HashLoan(&mut work.cognition_hash);
                self.with_factory_prologue_external_meter_v1(
                    common,
                    live_owner,
                    &mut loan,
                    |root, live, loan| {
                        *slot = Some(PendingCurrentCognitionJournalV1::from_accepted_at_head_v1(
                            parent.refs().cognition,
                            parent.refs().head,
                            root.ctx.transaction,
                            root.ctx.values.memory,
                            loan.0,
                            &live(),
                            &mut root.ctx.cancelled,
                        )?);
                        Ok(())
                    },
                )?;
            }
            let pending = slot.as_mut().ok_or_else(bad)?;
            let mut loan = HashLoan(&mut work.cognition_hash);
            let digest = self.with_factory_prologue_external_meter_v1(
                common,
                live_owner,
                &mut loan,
                |root, live, loan| {
                    pending.validate_evidence(
                        root.ctx.transaction,
                        root.ctx.values.memory,
                        &live(),
                    )?;
                    Ok(pending.current_digest(
                        root.ctx.values.memory,
                        loan.0,
                        &mut root.ctx.cancelled,
                    )?)
                },
            )?;
            heads[0] = CognitionHistoryHeadV1 {
                sequence: pending.sequence(),
                digest,
            };
            indexes[0] = CognitionHistoryIndexRefV1 {
                present: true,
                records: pending.count(),
                root: pending.index_root_data_v2().clone(),
            };
            self.check_final_cognition_head(
                parent,
                (CF::JournalHeadSeq, CF::JournalHeadDigest),
                &heads[0],
                true,
                common,
                live_owner,
            )?;
            self.check_final_cognition_head(
                parent,
                (CF::EconomyHeadSeq, CF::EconomyHeadDigest),
                &heads[1],
                indexes[2].present,
                common,
                live_owner,
            )?;
            self.check_final_cognition_head(
                parent,
                (CF::EconomyProvisionHeadSeq, CF::EconomyProvisionHeadDigest),
                &heads[2],
                indexes[3].present,
                common,
                live_owner,
            )?;
        } else if tick.cognition_for_final_source().is_some() {
            return Err(bad().into());
        }
        // Consume the actual frontier before the borrowed parent scope ends.
        // The resulting paid owner keeps the exact hash state alive for publication.
        let journal_frontier = if let Some(pending) = tick.cognition_for_final_source().take() {
            common.consume_steps(1).map_err(formal)?;
            let mut loan = HashLoan(&mut work.cognition_hash);
            Some(self.with_factory_prologue_external_meter_v1(
                common,
                live_owner,
                &mut loan,
                |root, live, loan| {
                    let written = pending.finish(
                        root.ctx.transaction,
                        loan.0,
                        &live(),
                        &mut root.ctx.cancelled,
                    )?;
                    Ok(written.retain_owned_v1(
                        root.ctx.transaction,
                        root.ctx.values.memory,
                        &live(),
                    )?)
                },
            )?)
        } else {
            None
        };
        let manifest = with_current_root_module_meter_v1(self, common, live_owner, |root, _| {
            super::super::incremental_module_execution_manifest_hash::prepare_current_world_manifest_hash_v1(root.families.parent,root.families.head,root.families.roots,root.manifest_cache,None,root.families.config,root.state,root.ctx,root.manifest_work).map_err(RootModuleExecutionErrorV1::Manifest)
        })?;
        // Reborrow the exact action bank after the tick. No second lifecycle,
        // factory or Common allowance is constructed for the original ordered
        // PolicySet source used by legacy semantic-state hashing.
        if actions.common_work().identity() != common.work().identity() {
            return Err(BodyMutationError::Invalid("normal policy source actual work Core").into());
        }
        let policy_parents=super::super::incremental_normal_source_action_views::bind_normal_action_source_parents_v1(
            parent,parent_caches.borrow(),common,
        )?;
        let current_policy=policy_parents.with_sources_v1(authorization,compiler,common,|sources,common|{
            super::super::incremental_module_capability_admission_rules::read_current_bootstrap_policy_v1(
                sources.module_lifecycle,self.ctx,parent_read,&mut actions.factory_schedule,
                &mut actions.module_lifecycle.view(),common,&live_owner(),
            )
        })?.map_err(|error|BodyMutationError::Original(Box::new(error)))?;
        let authorization = authorization
            .capture_normal_resident_v1(
                parent.refs().storage.current().runtime(),
                parent.refs().roots,
                self.ctx,
                parent_read,
                &mut work.authorization,
                &mut work.authorization_pending_entries,
                common,
                &live_owner(),
            )
            .map_err(|e| BodyMutationError::Original(Box::new(SourceCause(e))))?;
        let delivery = delivery
            .capture_normal_resident_v1(
                self.ctx.transaction,
                self.originals,
                &mut work.originals,
                common,
                &mut self.ctx.cancelled,
                &live_owner(),
            )
            .map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
        let ledger = match &tail.cognition {
            Some(source) => Some(source.ledger.capture_manifest_source_v1(
                self.ctx.transaction,
                common,
                &live_owner(),
            )?),
            None => None,
        };
        if parent.refs().storage.results().proof_ledger().is_some() != ledger.is_some() {
            return Err(bad().into());
        }
        let business_delta = self.capture_normal_business_delta_v1(
            parent,
            revocation,
            parent_read,
            &mut work.business,
            common,
            live_owner,
        )?;
        let terminal_metadata = self.capture_normal_terminal_metadata_v1(
            parent,
            business_parent,
            revocation,
            &journal,
            &event_commitment,
            &manifest,
            tail.cognition.as_ref().map(|s| s.ledger),
            parent_read,
            &mut work.business,
            &mut work.terminal_metadata,
            common,
            live_owner,
        )?;
        let retention = retention.freeze(parent, self.ctx.transaction, common, &live_owner())?;
        let cognition_view = CapturedCognitionViewV1 {
            guard: self.ctx.transaction.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            parent_fence: parent.fence(),
            parent_record: parent.refs().head.verified_record_id(),
            enabled,
            revision: old.revision.checked_add(1).ok_or_else(bad)?,
            previous: parent.refs().cognition.selected().occurrence().location(),
            heads,
            indexes,
        };
        let cognition_projection = NormalCognitionProjectionInputsV1 {
            retention: &retention,
            view: &cognition_view,
            ledger: ledger.as_ref(),
            parent_ledger: tail.cognition.as_ref().map(|source| source.accepted_ledger),
            journal_frontier: journal_frontier.as_ref(),
            economy_frontier: economy_frontier.as_ref(),
        };
        cognition_projection.validate(self.ctx.transaction, common, &live_owner())?;
        let projection = self
            .prepare_current_normal_projection_commitment_v1(
                parent,
                projection_parent,
                revocation,
                &terminal_metadata,
                &event_commitment,
                &cognition_projection,
                &authorization,
                &current_policy,
                process,
                parent_read,
                &mut work.projection.loan(),
                common,
                live_owner,
            )?
            .freeze_v1(self.ctx.transaction, common, &live_owner())?;
        let after =
            capture_normal_write_fact_cut_v1(self.ctx.transaction, cut_work, common, live_owner)?;
        if before != after {
            return Err(
                BodyMutationError::Invalid("normal final capture changed state/facts").into(),
            );
        }
        let bytes = std::mem::size_of::<CapturedNormalFinalSourcesV1>()
            + std::mem::size_of::<ReservedValue<CapturedNormalFinalSourcesV1>>()
            + 2 * std::mem::size_of::<usize>()
            + 4 * std::mem::align_of::<ReservedValue<CapturedNormalFinalSourcesV1>>()
            + parent.refs().roots.world_id.len()
            + 8 * 512;
        common.charge(1, bytes as u64).map_err(formal)?;
        let mut fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        fee.merge(&mut metadata_fee)
            .map_err(BodyMutationError::Invalid)?;
        let mut world_id = String::new();
        world_id
            .try_reserve_exact(parent.refs().roots.world_id.len())
            .map_err(|_| bad())?;
        if world_id.capacity() > parent.refs().roots.world_id.len() {
            return Err(bad().into());
        }
        world_id.push_str(&parent.refs().roots.world_id);
        let owner = CapturedNormalFinalSourcesV1 {
            retention,
            projection,
            business_delta,
            terminal_metadata,
            event_commitment,
            runtime: [snapshot, journal, ticks, audit],
            journal_frontier,
            economy_frontier,
            authorization,
            delivery,
            manifest,
            previous_runtime: history.occurrence().location().original,
            max_snapshots: history.manifest().max_snapshots,
            archived_ticks: history.manifest().archived_tick_count,
            cognition: cognition_view,
            ledger,
            before_guard,
            after_guard: self.ctx.transaction.guard(),
            core: common.work().identity(),
            now: self.now,
            world_id,
            _fee: fee,
        };
        // Arc's true control block and all copied fixed metadata were admitted above.
        let outer = common
            .memory()
            .reserve(
                std::mem::size_of::<CapturedNormalFinalSourcesV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(Arc::new(outer.retain(owner)))
    }
}

#[derive(Debug)]
struct SourceCause<T: std::fmt::Debug>(T);
impl<T: std::fmt::Debug> std::fmt::Display for SourceCause<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl<T: std::fmt::Debug> std::error::Error for SourceCause<T> {}

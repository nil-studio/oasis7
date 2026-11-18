//! Actual checkpoint C plus nonempty original (C,T] suffix. Only C enters the
//! full Recovery qualifier; every successor inherits its preceding paid source.
use super::cold_install::{
    AuthenticatedColdHostErrorV1, ColdParentReadRefsV1, ColdParentSourcesV1,
    finish_authenticated_cold_parent_bounded_v1,
};
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::normal_candidate_context_bounded,
    incremental_execution::ExecutionError,
    incremental_history::{CatalogQueryV1, HISTORY_MAX_HEADER_BYTES},
    incremental_material::{CheckedCommitCore, FactLocationV1},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

/// Untrusted location of an earlier checkpoint in the actual current signed
/// native history. Positions are Formal positions, so genesis has position 1.
#[derive(Clone, Copy)]
pub struct IwsColdCheckpointSuffixV1 {
    pub checkpoint_position: u64,
    pub checkpoint_hints: IwsColdParentHintsV1,
}

struct Target {
    checkpoint: super::cold_checkpoint::AuthenticatedColdCheckpointV1,
    core: ReservedValue<CheckedCommitCore>,
}

/// An old body is selected through T's actual authenticated catalog prefix,
/// independently of its native proof. This never walks an expanded history.
fn check_catalog_member(
    refs: &ColdParentReadRefsV1<'_>,
    target: &Target,
    core: &ReservedValue<CheckedCommitCore>,
    work: &mut IwsColdParentWorkV1,
    source: &mut super::work::IwsCandidateSourceWorkV1,
    common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
) -> Result<(), FormalError> {
    if !target.core.uses_budget(common.memory()) || !core.uses_budget(common.memory()) {
        return Err(invalid("suffix catalog original Core pool"));
    }
    let pages = &refs
        .endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("suffix actual catalog pages"))?
        .pages;
    let memory = pages.memory_budget();
    super::cold_io::run_material(work, source, common, |work, _, metadata, cancelled| {
        metadata.charge(16, (8 * HISTORY_MAX_HEADER_BYTES) as u64)?;
        let _phase = memory
            .reserve(8 * HISTORY_MAX_HEADER_BYTES, MemoryClass::Ordinary)
            .map_err(|_| invalid("suffix catalog header admission"))?;
        let physical = std::sync::atomic::AtomicBool::new(false);
        let state = super::cold_page_loans::StateLoan::new(
            pages,
            &mut work.state_ordinary,
            MemoryClass::Ordinary,
            &physical,
            None,
        )
        .map_err(page_store_formal_error)?;
        let mut first = None;
        let originals = {
            let mut meter = |steps, bytes| {
                if cancelled() {
                    physical.store(true, std::sync::atomic::Ordering::Release);
                    if first.is_none() {
                        first = Some(invalid("suffix catalog cancelled"));
                    }
                    return Err(ExecutionError::Invalid("suffix catalog stopped"));
                }
                metadata.charge(steps, bytes).map_err(|error| {
                    if first.is_none() {
                        first = Some(error);
                    }
                    ExecutionError::Invalid("suffix catalog work exhausted")
                })
            };
            (|| {
                let selected = target.core.catalog().before.lookup_with_work(
                    CatalogQueryV1::Height(core.body().height),
                    state.source(),
                    &mut meter,
                )?;
                let body = core.body().digest_with_work(&mut meter)?;
                let checkpoint = core
                    .checkpoint()
                    .map(|checkpoint| checkpoint.digest_with_work(&mut meter))
                    .transpose()?;
                Ok::<_, ExecutionError>((selected, body, checkpoint))
            })()
        };
        if let Some(error) = first {
            return Err(error);
        }
        if let Some(error) = state.source().take_read_error() {
            return Err(page_store_formal_error(error));
        }
        let (selected, body, checkpoint) = originals
            .map_err(|e| invalid(format!("suffix authenticated catalog originals: {e:?}")))?;
        let selected = selected.ok_or_else(|| invalid("suffix target catalog member missing"))?;
        if selected.commit != body
            || selected.parent_commit != core.body().parent_commit
            || selected.scope != core.body().scope
            || selected.checkpoint.map(|pointer| pointer.descriptor) != checkpoint
            || selected.checkpoint.map(|pointer| pointer.height)
                != core.checkpoint().map(|checkpoint| checkpoint.height)
        {
            return Err(invalid("suffix exact catalog body/checkpoint binding"));
        }
        Ok(())
    })
}

fn recover_sources<L>(
    refs: &ColdParentReadRefsV1<'_>,
    selection: IwsColdCheckpointSuffixV1,
    work: &mut IwsColdParentWorkV1,
    source: &mut super::work::IwsCandidateSourceWorkV1,
    mailbox_hint: &mut L,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ColdParentSourcesV1, AuthenticatedColdHostErrorV1>
where
    L: FnMut(
        &[u8; 32],
    ) -> Result<oasis7_formal_protocol::incremental_material::FactLocationV1, String>,
{
    let (position, target_position) = {
        let mut common = refs.archive.bounded_context(refs.memory, stop);
        common.consume_steps(1)?;
        let position = selection.checkpoint_position;
        let target_position = refs.endpoint.store.body.head.position;
        if position == 0 || position >= target_position {
            return Err(invalid("checkpoint suffix requires C < T").into());
        }
        (position, target_position)
    };
    let target_checkpoint = super::cold_checkpoint::recover_authenticated_checkpoint_bounded_v1(
        refs.endpoint,
        refs.lease,
        refs.pinned,
        refs.memory,
        refs.archive,
        stop,
    )?;
    let mut target = {
        let mut common = refs.archive.bounded_context(refs.memory, stop);
        let target_core = super::cold_core::recover_ordinary_core_v1(
            refs.endpoint,
            &target_checkpoint,
            work,
            &mut common,
        )?;
        Some(Target {
            checkpoint: target_checkpoint,
            core: target_core,
        })
    };
    let checkpoint = super::cold_checkpoint::recover_authenticated_checkpoint_at_bounded_v1(
        refs.endpoint,
        refs.lease,
        refs.pinned,
        position,
        refs.memory,
        refs.archive,
        stop,
    )?;
    let (mut material, mut parent) = {
        let mut common = refs.archive.bounded_context(refs.memory, stop);
        let core = super::cold_core::recover_ordinary_core_v1(
            refs.endpoint,
            &checkpoint,
            work,
            &mut common,
        )?;
        if core.checkpoint().is_none() {
            return Err(invalid("suffix C must contain an original checkpoint descriptor").into());
        }
        check_catalog_member(
            refs,
            target
                .as_ref()
                .ok_or_else(|| invalid("suffix actual target missing"))?,
            &core,
            work,
            source,
            &mut common,
        )?;
        let pages = &refs
            .endpoint
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("suffix actual pages"))?
            .pages;
        // This is the only complete state/history recovery in the suffix path.
        // Neither T nor an intermediate child can enter this full qualifier.
        // Share the ordinary Cold/Ready locator type while borrowing its owner.
        let hint: &mut dyn FnMut(&[u8; 32]) -> Result<FactLocationV1, String> = &mut *mailbox_hint;
        let recovered = super::cold_recovery_paid::recover_complete_sources_bounded_v1(
            pages,
            &checkpoint,
            &core,
            work,
            selection.checkpoint_hints,
            hint,
            &mut common,
        )?;
        let ordinary = super::cold_readmission_paid::readmit_complete_sources_bounded_v1(
            pages,
            &checkpoint,
            &recovered,
            &core,
            work,
            &mut common,
        )?;
        drop(recovered);
        let material = super::cold_execution_material::recover_execution_material_bounded_v1(
            refs.endpoint,
            &checkpoint,
            &core,
            work,
            source,
            &mut common,
        )?;
        let parent = ColdParentSourcesV1 {
            checkpoint,
            core,
            ordinary,
        };
        (material, parent)
    };
    let mut next_position = position;
    while next_position < target_position {
        {
            let mut common = refs.archive.bounded_context(refs.memory, stop);
            common.consume_steps(1)?;
            next_position = next_position
                .checked_add(1)
                .ok_or_else(|| invalid("suffix position overflow"))?;
        }
        let next = if next_position == target_position {
            target
                .take()
                .ok_or_else(|| invalid("suffix actual target already consumed"))?
        } else {
            let checkpoint =
                super::cold_checkpoint::recover_authenticated_checkpoint_at_bounded_v1(
                    refs.endpoint,
                    refs.lease,
                    refs.pinned,
                    next_position,
                    refs.memory,
                    refs.archive,
                    stop,
                )?;
            let mut common = refs.archive.bounded_context(refs.memory, stop);
            let core = super::cold_core::recover_ordinary_core_v1(
                refs.endpoint,
                &checkpoint,
                work,
                &mut common,
            )?;
            check_catalog_member(
                refs,
                target
                    .as_ref()
                    .ok_or_else(|| invalid("suffix target missing"))?,
                &core,
                work,
                source,
                &mut common,
            )?;
            Target { checkpoint, core }
        };
        let mut common = refs.archive.bounded_context(refs.memory, stop);
        let FormalRequestStateV1::Qualified {
            candidate, proof, ..
        } = next.checkpoint.state()
        else {
            return Err(invalid("suffix original accepted state").into());
        };
        if candidate.branch != FormalCompletionBranchV1::Normal
            || proof.decision.branch != FormalCompletionBranchV1::Normal
            || next.checkpoint.head().record().position != next_position
            || next.core.roots().height.checked_add(1) != Some(next_position)
        {
            return Err(invalid("suffix same-authority normal lineage required").into());
        }
        let frozen = candidate
            .frozen
            .as_ref()
            .ok_or_else(|| invalid("suffix original normal Frozen certificate"))?;
        let context = normal_candidate_context_bounded(
            &candidate.candidate,
            frozen,
            &refs.endpoint.authority,
            parent.checkpoint.head(),
            &mut common,
        )?;
        // Every historical step needs its own original frozen/native/artifact
        // material. The final T is checked by the common finisher below; an
        // intermediate version cannot be silently filled from T's artifacts.
        let next_material = if next_position < target_position {
            Some(
                super::cold_execution_material::recover_execution_material_bounded_v1(
                    refs.endpoint,
                    &next.checkpoint,
                    &next.core,
                    work,
                    source,
                    &mut common,
                )?,
            )
        } else {
            None
        };
        let ordinary = super::cold_suffix_step::advance_suffix_sources_bounded_v1(
            super::cold_suffix_step::SuffixStepRefsV1 {
                endpoint: refs.endpoint,
                parent: &parent,
                checkpoint: &next.checkpoint,
                core: &next.core,
                context: &context,
            },
            work,
            source,
            mailbox_hint,
            &mut common,
        )?;
        // The preceding paid source is released only after its exact child has
        // obtained every ordinary qualification. No expanded suffix is held.
        parent = ColdParentSourcesV1 {
            checkpoint: next.checkpoint,
            core: next.core,
            ordinary,
        };
        if let Some(next_material) = next_material {
            drop(std::mem::replace(&mut material, next_material));
        }
    }
    let mut common = refs.archive.bounded_context(refs.memory, stop);
    common.consume_steps(0)?;
    Ok(parent)
}

impl FileFormalEndpoint {
    /// Recover an earlier real checkpoint and every original accepted suffix
    /// step before the ordinary actual-T native/local closure installation.
    pub fn into_authenticated_cold_host_source_from_checkpoint_bounded<L>(
        mut self,
        publication: FormalEndpointPublicationRefsV1<'_>,
        checkpoint: IwsColdCheckpointSuffixV1,
        work: &mut IwsColdParentWorkV1,
        source: &mut super::work::IwsCandidateSourceWorkV1,
        mut mailbox_hint: L,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<super::host_source::PaidAcceptedGenesisHostSourceV1, AuthenticatedColdHostErrorV1>
    where
        L: FnMut(
            &[u8; 32],
        )
            -> Result<oasis7_formal_protocol::incremental_material::FactLocationV1, String>,
    {
        let FormalEndpointPublicationRefsV1 {
            provider,
            policy,
            memory,
            work: archive,
        } = publication;
        self.store.reconcile_authenticated_cold_scope_bounded_v1(
            &self.authority,
            self.role,
            publication,
            stop,
        )?;
        let lease = {
            let instance = match self.role {
                EndpointRole::Primary => &self.authority.definition().primary.instance,
                EndpointRole::Replica => &self.authority.definition().replica.instance,
            };
            let mut common = archive.bounded_context(memory, stop);
            crate::controlled_authority::fencing::acquire_provider_lease_bounded_v1(
                provider,
                instance,
                &self.store.checkpoint,
                policy,
                &mut common,
            )?
        };
        let refs = ColdParentReadRefsV1 {
            endpoint: &self,
            lease: &lease,
            pinned: policy,
            memory,
            archive,
        };
        let inputs = recover_sources(&refs, checkpoint, work, source, &mut mailbox_hint, stop)?;
        let (cache, head) =
            finish_authenticated_cold_parent_bounded_v1(refs, inputs, work, source, stop)?;
        self.iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("cold page capability removed"))?
            .jobs
            .restore_cold(cache, &head)?;
        self.parent = Some(head);
        self.into_authenticated_cold_host_source_with_lease_bounded(
            lease, provider, policy, memory, archive, stop,
        )
        .map_err(AuthenticatedColdHostErrorV1::from)
    }
}

//! Chain-only normal execution capability. Serializable Consensus data is
//! deliberately insufficient: this scope borrows both real Frozen FD holders
//! and original Node fresh/restored provenance for the whole runtime call.
use crate::iws_normal_input_origin::{NodeOriginContextErrorV1, PaidNodeInputOriginV1};
use oasis7_consensus::normal_tick_input::{
    PaidBoundNormalInputDataV1, bind_normal_input_data_bounded,
};
use oasis7_distfs::controlled_authority::formal::endpoint::FormalArchiveReadWorkV1;
use oasis7_distfs::controlled_authority::formal::endpoint::PaidNormalFrozenPairV1;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError, IncrementalFrozenMaterialWorkV1,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    context_wire,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryLimitExceeded, ReservedValue};

/// Prepaid before any Frozen/Node owner is consumed. This is only memory/Core
/// identity, never source provenance or execution authority.
pub(crate) struct PaidNormalTickSourcesAdmissionV1 {
    core: WorkIdentityV1,
    fee: ReservedValue<()>,
}
impl PaidNormalTickSourcesAdmissionV1 {
    pub(crate) fn reserve(ctx: &FormalBoundedContextV1<'_>) -> Result<Self, MemoryLimitExceeded> {
        let bytes = std::mem::size_of::<
            Result<PaidNormalTickSourcesV1, NormalTickSourcesAdmissionFailureV1>,
        >()
        .max(std::mem::size_of::<Self>());
        Ok(Self {
            core: ctx.work().identity(),
            fee: ctx
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)?
                .retain(()),
        })
    }

    fn matches_context(&self, ctx: &FormalBoundedContextV1<'_>) -> bool {
        self.core == ctx.work().identity() && self.fee.uses_budget(ctx.memory())
    }
}

/// No dynamic error payload can outlive its admission. The prepaid Result
/// layout covers every cause, including when the remaining pool is exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NormalTickSourcesAdmissionCauseV1 {
    Node(NodeOriginContextErrorV1),
    FrozenForeignPool,
    AdmissionForeignContext,
}

pub(crate) struct PaidNormalTickSourcesV1 {
    frozen: PaidNormalFrozenPairV1,
    node: PaidNodeInputOriginV1,
    _outer: ReservedValue<()>,
}
/// Admission failure retains the exact Frozen endpoints and captured Node
/// provenance. Retrying cannot supply a replacement pair or a new Node origin.
pub(crate) struct NormalTickSourcesAdmissionFailureV1 {
    frozen: PaidNormalFrozenPairV1,
    node: PaidNodeInputOriginV1,
    admission: PaidNormalTickSourcesAdmissionV1,
    cause: NormalTickSourcesAdmissionCauseV1,
}
impl NormalTickSourcesAdmissionFailureV1 {
    pub(crate) fn cause(&self) -> &NormalTickSourcesAdmissionCauseV1 {
        &self.cause
    }

    pub(crate) fn recover_bounded(
        self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<PaidNormalTickSourcesV1, Self> {
        PaidNormalTickSourcesV1::from_captured_node(self.admission, self.frozen, self.node, ctx)
    }
}
pub(crate) struct PaidVerifiedNormalTickInputV1<'a> {
    data: PaidBoundNormalInputDataV1<'a>,
    source: &'a PaidNormalTickSourcesV1,
    _outer: ReservedValue<()>,
}
impl PaidNormalTickSourcesV1 {
    /// Borrow the retained real dual-Frozen owner after its Host slots moved.
    /// This does not reconstruct a parent from Node DATA or release its leases.
    pub(crate) fn frozen_sources(&self) -> &PaidNormalFrozenPairV1 {
        &self.frozen
    }
    /// Chain's fixed publication phase keeps the original Node provenance and
    /// its paid shell beside the consumed dual-Frozen/Ready recovery owner.
    pub(crate) fn into_publication_parts(
        self,
    ) -> (
        PaidNormalFrozenPairV1,
        PaidNodeInputOriginV1,
        ReservedValue<()>,
    ) {
        (self.frozen, self.node, self._outer)
    }
    /// Capture Node provenance inside its actual fresh/restored callback before
    /// moving Frozen here. Capture errors leave the driver's pair untouched;
    /// reserve the success/failure carrier while both owners are still outside
    /// this call. Every consuming outcome keeps that same prepaid carrier.
    pub(crate) fn from_captured_node(
        admission: PaidNormalTickSourcesAdmissionV1,
        frozen: PaidNormalFrozenPairV1,
        node: PaidNodeInputOriginV1,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, NormalTickSourcesAdmissionFailureV1> {
        let checked = (|| {
            node.validate_context_typed(ctx)
                .map_err(NormalTickSourcesAdmissionCauseV1::Node)?;
            if !frozen.uses_budget(ctx.memory()) {
                return Err(NormalTickSourcesAdmissionCauseV1::FrozenForeignPool);
            }
            if !admission.matches_context(ctx) {
                return Err(NormalTickSourcesAdmissionCauseV1::AdmissionForeignContext);
            }
            Ok(())
        })();
        match checked {
            Ok(()) => Ok(Self {
                frozen,
                node,
                _outer: admission.fee,
            }),
            Err(cause) => Err(NormalTickSourcesAdmissionFailureV1 {
                frozen,
                node,
                admission,
                cause,
            }),
        }
    }
    pub(crate) fn input<'a>(
        &'a self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        material_work: &mut IncrementalFrozenMaterialWorkV1,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<PaidVerifiedNormalTickInputV1<'a>, FormalError> {
        let checked = self
            .frozen
            .check_material(memory, work, material_work, stop)?;
        let mut ctx = work.bounded_context(memory, stop);
        self.node.validate_context(&mut ctx)?;
        if !self._outer.uses_budget(memory) {
            return Err(FormalError::Invalid("normal sources pool".into()));
        }
        let data = bind_normal_input_data_bounded(checked, &mut ctx)?;
        // Compare the original Node bytes to those actually authenticated by
        // Frozen. No caller supplied ID, restored flag or content hash grants
        // permission. Both physical source/FD holders remain in self.
        let context = context_wire::encode_paid(
            data.context(),
            64 << 20,
            &mut ctx,
            "normal input bound",
            "normal Node context",
        )?;
        let continuation = context_wire::encode_paid(
            data.continuation(),
            64 << 20,
            &mut ctx,
            "normal continuation bound",
            "normal Node continuation",
        )?;
        for (actual, original) in [
            (context.as_slice(), self.node.input_bytes()),
            (continuation.as_slice(), self.node.continuation_bytes()),
        ] {
            ctx.charge(
                1,
                u64::try_from(actual.len())
                    .map_err(|_| FormalError::Invalid("normal Node wire length".into()))?,
            )?;
            if actual != original {
                return Err(FormalError::Invalid(
                    "normal Frozen/Node original mismatch".into(),
                ));
            }
        }
        let fee = memory
            .reserve(
                std::mem::size_of::<PaidVerifiedNormalTickInputV1<'a>>(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| FormalError::Invalid("normal tick input admission".into()))?;
        Ok(PaidVerifiedNormalTickInputV1 {
            data,
            source: self,
            _outer: fee.retain(()),
        })
    }
}

impl PaidVerifiedNormalTickInputV1<'_> {
    pub(crate) fn frozen_sources(&self) -> &PaidNormalFrozenPairV1 {
        &self.source.frozen
    }
    pub(crate) fn data(&self) -> &PaidBoundNormalInputDataV1<'_> {
        &self.data
    }
    pub(crate) fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        self.source.node.validate_context(ctx)?;
        self.data.validate_context(ctx)?;
        if !self.source.frozen.uses_budget(ctx.memory()) || !self._outer.uses_budget(ctx.memory()) {
            return Err(FormalError::Invalid("normal tick input pool".into()));
        }
        Ok(())
    }
    /// Diagnostic Node origin only. Product-validation resume still requires
    /// its independent retained intent/cursor and cannot be selected here.
    pub(crate) fn node_restored(&self) -> bool {
        self.source.node.is_node_restored()
    }
}

#[cfg(test)]
mod owned_sources_tests {
    use super::*;

    #[test]
    fn normal_sources_and_admission_failure_can_outlive_the_node_callback() {
        fn assert_owned<T: Send + 'static>() {}
        assert_owned::<PaidNormalTickSourcesAdmissionV1>();
        assert_owned::<PaidNodeInputOriginV1>();
        assert_owned::<PaidNormalTickSourcesV1>();
        assert_owned::<NormalTickSourcesAdmissionFailureV1>();
        assert!(!std::mem::needs_drop::<NormalTickSourcesAdmissionCauseV1>());
    }

    #[test]
    fn admission_prepays_complete_result_without_consuming_sources_or_core() {
        use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
        let bytes = std::mem::size_of::<
            Result<PaidNormalTickSourcesV1, NormalTickSourcesAdmissionFailureV1>,
        >()
        .max(std::mem::size_of::<PaidNormalTickSourcesAdmissionV1>());
        let memory = MemoryBudget::new(bytes, 0).unwrap();
        let work = FormalBoundedWorkV1::new(10, 20).unwrap();
        let mut stop = || panic!("memory prepayment must not execute a work callback");
        let ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let admission = PaidNormalTickSourcesAdmissionV1::reserve(&ctx).unwrap();
        assert_eq!(memory.used(), bytes);
        assert!(admission.matches_context(&ctx));
        let error = PaidNormalTickSourcesAdmissionV1::reserve(&ctx)
            .err()
            .unwrap();
        assert_eq!(error.requested, bytes);
        assert_eq!(error.used, bytes);
        assert_eq!(error.admission_limit, bytes);
        assert_eq!(
            (work.remaining_steps(), work.remaining_read_bytes()),
            (10, 20)
        );
        // A prepaid carrier stays usable when no more Ordinary memory remains.
        assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
        assert!(admission.matches_context(&ctx));
        drop(admission);
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn admission_cannot_rebind_equal_balance_core_or_another_pool() {
        use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let foreign_pool = MemoryBudget::new(1 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(10, 20).unwrap();
        let foreign_work = FormalBoundedWorkV1::new(10, 20).unwrap();
        let mut stop = || false;
        let ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let admission = PaidNormalTickSourcesAdmissionV1::reserve(&ctx).unwrap();
        let paid = memory.used();
        assert!(paid > 0);
        assert!(!admission.matches_context(&FormalBoundedContextV1::new(
            &foreign_pool,
            &work,
            &mut || false,
        )));
        assert!(!admission.matches_context(&FormalBoundedContextV1::new(
            &memory,
            &foreign_work,
            &mut || false,
        )));
        assert!(admission.matches_context(&ctx));
        assert_eq!(memory.used(), paid);
        assert_eq!(foreign_pool.used(), 0);
        assert_eq!(
            (work.remaining_steps(), work.remaining_read_bytes()),
            (10, 20)
        );
        assert_eq!(
            (
                foreign_work.remaining_steps(),
                foreign_work.remaining_read_bytes()
            ),
            (10, 20)
        );
        drop(admission);
        assert_eq!(memory.used(), 0);
    }
}

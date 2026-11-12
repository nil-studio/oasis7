//! Chain-only normal execution capability. Serializable Consensus data is
//! deliberately insufficient: this scope borrows both real Frozen FD holders
//! and original Node fresh/restored provenance for the whole runtime call.
use crate::iws_normal_input_origin::PaidNodeInputOriginV1;
use oasis7_consensus::normal_tick_input::{
    PaidBoundNormalInputDataV1, bind_normal_input_data_bounded,
};
use oasis7_distfs::controlled_authority::formal::endpoint::FormalArchiveReadWorkV1;
use oasis7_distfs::controlled_authority::formal::endpoint::PaidNormalFrozenPairV1;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError, IncrementalFrozenMaterialWorkV1, bounded_context::FormalBoundedContextV1,
    context_wire,
};
use oasis7_node::{VerifiedNodeLocalContinuationV1, VerifiedNodeRestoredContinuationV1};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub(crate) struct PaidNormalTickSourcesV1 {
    frozen: PaidNormalFrozenPairV1,
    node: PaidNodeInputOriginV1,
    _outer: ReservedValue<()>,
}
pub(crate) struct PaidVerifiedNormalTickInputV1<'a> {
    data: PaidBoundNormalInputDataV1<'a>,
    source: &'a PaidNormalTickSourcesV1,
    _outer: ReservedValue<()>,
}
impl PaidNormalTickSourcesV1 {
    pub(crate) fn fresh(
        frozen: PaidNormalFrozenPairV1,
        node: &VerifiedNodeLocalContinuationV1<'_>,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, FormalError> {
        let node = PaidNodeInputOriginV1::fresh(node, ctx)?;
        Self::new(frozen, node, ctx)
    }
    pub(crate) fn restored(
        frozen: PaidNormalFrozenPairV1,
        node: &VerifiedNodeRestoredContinuationV1<'_>,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, FormalError> {
        let node = PaidNodeInputOriginV1::restored(node, ctx)?;
        Self::new(frozen, node, ctx)
    }
    fn new(
        frozen: PaidNormalFrozenPairV1,
        node: PaidNodeInputOriginV1,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, FormalError> {
        node.validate_context(ctx)?;
        if !frozen.uses_budget(ctx.memory()) {
            return Err(FormalError::Invalid("normal Frozen source pool".into()));
        }
        let fee = ctx
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(|_| FormalError::Invalid("normal tick sources admission".into()))?;
        Ok(Self {
            frozen,
            node,
            _outer: fee.retain(()),
        })
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

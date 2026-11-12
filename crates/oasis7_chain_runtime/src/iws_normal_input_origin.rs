//! Node-origin bytes captured only from the actual fresh/restored Node hooks.
//! Recovery here is Node recovery; product-validation resume is independent.
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    context_wire,
};
use oasis7_node::{VerifiedNodeLocalContinuationV1, VerifiedNodeRestoredContinuationV1};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

enum NodeOrigin {
    Fresh,
    Restored,
}
pub(crate) struct PaidNodeInputOriginV1 {
    input: ReservedValue<Vec<u8>>,
    continuation: ReservedValue<Vec<u8>>,
    origin: NodeOrigin,
    identity: WorkIdentityV1,
    _outer: ReservedValue<()>,
}
impl PaidNodeInputOriginV1 {
    pub(crate) fn fresh(
        source: &VerifiedNodeLocalContinuationV1<'_>,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, FormalError> {
        Self::capture(source.original(), NodeOrigin::Fresh, ctx)
    }
    pub(crate) fn restored(
        source: &VerifiedNodeRestoredContinuationV1<'_>,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, FormalError> {
        Self::capture(source.original(), NodeOrigin::Restored, ctx)
    }
    fn capture(
        source: &oasis7_node::NodeLocalExecutionContinuation,
        origin: NodeOrigin,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, FormalError> {
        ctx.consume_steps(1)?;
        let outer = ctx
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(|_| FormalError::Invalid("normal Node origin admission".into()))?;
        let input = context_wire::encode_paid(
            &source.context,
            64 << 20,
            ctx,
            "normal input bound",
            "Node committed context",
        )?;
        let continuation = context_wire::encode_paid(
            source,
            64 << 20,
            ctx,
            "normal continuation bound",
            "Node continuation",
        )?;
        ctx.consume_steps(1)?;
        Ok(Self {
            input,
            continuation,
            origin,
            identity: ctx.work().identity(),
            _outer: outer.retain(()),
        })
    }
    pub(crate) fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        if self.identity != ctx.work().identity() || !self.uses_budget(ctx.memory()) {
            return Err(FormalError::Invalid("normal Node origin pool/Core".into()));
        }
        Ok(())
    }
    pub(crate) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.input.uses_budget(memory)
            && self.continuation.uses_budget(memory)
            && self._outer.uses_budget(memory)
    }
    pub(crate) fn input_bytes(&self) -> &[u8] {
        &self.input
    }
    pub(crate) fn continuation_bytes(&self) -> &[u8] {
        &self.continuation
    }
    pub(crate) fn is_node_restored(&self) -> bool {
        matches!(self.origin, NodeOrigin::Restored)
    }
}

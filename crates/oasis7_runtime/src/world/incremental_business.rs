//! Borrow original Runtime business state for the shared typed persistence path.
use super::{LogisticsSlaMetrics, World};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_runtime_business::prepare_runtime_business_genesis_v1,
};
impl World {
    pub fn prepare_iws_runtime_business_genesis_v1<
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        remaining: &mut u64,
        sink: &mut S,
    ) -> Result<(), MemberRecordError> {
        let LogisticsSlaMetrics {
            completed_transits,
            fulfilled_transits,
            breached_transits,
            total_delay_ticks,
            urgent_completed_transits,
            urgent_fulfilled_transits,
            urgent_breached_transits,
            urgent_total_delay_ticks,
        } = self.logistics_sla_metrics;
        let metrics = [
            completed_transits,
            fulfilled_transits,
            breached_transits,
            total_delay_ticks,
            urgent_completed_transits,
            urgent_fulfilled_transits,
            urgent_breached_transits,
            urgent_total_delay_ticks,
        ];
        prepare_runtime_business_genesis_v1(&metrics, &self.threat_heatmap, ctx, remaining, sink)
    }
}

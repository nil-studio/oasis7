//! One original bounded wire encoder bridged to the same Common Core.
use super::bounded_context::FormalBoundedContextV1;
use super::{crypto::invalid, types::FormalError};
use oasis7_proto::memory_budget::ReservedValue;
pub fn encode_paid<T: serde::Serialize>(
    value: &T,
    max: usize,
    ctx: &mut FormalBoundedContextV1<'_>,
    wirelimitmsg: &str,
    encodingprefix: &str,
) -> Result<ReservedValue<Vec<u8>>, FormalError> {
    use super::bounded_wire::{BoundedWireErrorV1, BoundedWireWorkV1, encode_bounded_wire_v1};
    ctx.consume_steps(1)?;
    let (memory, work, stop) = ctx.parts();
    let lease = work.begin_child()?;
    let before = (lease.remaining_steps(), lease.remaining_read_bytes());
    let mut scalar = BoundedWireWorkV1 {
        max_wire_bytes: max,
        remaining_steps: before.0,
        remaining_bytes: before.1,
    };
    let result =
        encode_bounded_wire_v1(value, memory, &mut scalar, &mut || stop()).map_err(|e| match e {
            BoundedWireErrorV1::Cancelled => invalid("bounded archive read cancelled"),
            BoundedWireErrorV1::StepLimit => invalid("bounded archive read steps"),
            BoundedWireErrorV1::ByteLimit => invalid("bounded archive read bytes"),
            BoundedWireErrorV1::WireLimit => invalid(wirelimitmsg),
            other => invalid(format!("{encodingprefix}: {other}")),
        });
    let delta = before
        .0
        .checked_sub(scalar.remaining_steps)
        .zip(before.1.checked_sub(scalar.remaining_bytes));
    let debit = match delta {
        Some((s, b)) => lease.consume(s, b),
        None => {
            drop(lease);
            return result.and_then(|_| Err(invalid("bounded archive child counters increased")));
        }
    };
    lease.finish();
    match result {
        Err(e) => Err(e),
        Ok(value) => {
            debit?;
            Ok(value)
        }
    }
}

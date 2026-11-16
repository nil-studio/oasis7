//! Genuine borrowed original envelope producer; absence is explicit DATA and
//! gains acceptance only through the independent signed role check.
use super::*;
use crate::incremental_cognition_fields::CognitionEnvelopeRefV1;
pub fn prepare_cognition_retention_ledger_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    envelope: Option<CognitionEnvelopeRefV1<'_>>,
    roots: [u8; 32],
    write: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<RetentionLedgerManifestV1>, ValueTreeError> {
    common.consume_steps(1).map_err(formal)?;
    if !write.memory.same_pool(common.memory()) {
        return Err(invalid());
    }
    let payload = match envelope.and_then(|e| e.field("retention_state")) {
        None => None,
        Some(value) => Some(genesis_payload(value, write, work, common)?),
    };
    common
        .charge(1, (size_of::<RetentionLedgerManifestV1>() + 512) as u64)
        .map_err(formal)?;
    let fee = common.memory().reserve(
        size_of::<RetentionLedgerManifestV1>() + 512,
        MemoryClass::Ordinary,
    )?;
    Ok(fee.retain(RetentionLedgerManifestV1 {
        roots,
        previous_record: None,
        payload,
    }))
}
fn genesis_payload<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    value: &serde_json::Value,
    write: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<StateRootV1, ValueTreeError> {
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child().map_err(formal)?;
    let before = [
        work.producer.cells_remaining,
        work.producer.json_nodes_remaining,
        write.io.nodes_remaining,
        write.io.read_bytes_remaining,
        write.io.write_bytes_remaining,
        write.key_work.bytes_remaining,
    ];
    let mut cap = before;
    for n in &mut cap[..3] {
        *n = (*n).min(child.remaining_steps() / 3)
    }
    for n in &mut cap[3..] {
        *n = (*n).min(child.remaining_read_bytes() / 3)
    }
    work.producer.cells_remaining = cap[0];
    work.producer.json_nodes_remaining = cap[1];
    write.io.nodes_remaining = cap[2];
    write.io.read_bytes_remaining = cap[3];
    write.io.write_bytes_remaining = cap[4];
    write.key_work.bytes_remaining = cap[5];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let caller_stop = &mut write.cancelled;
        let mut output = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || caller_stop() || stop(),
        };
        prepare_json_subtree_v1(profile(), value, &mut output, &mut work.producer).map_err(member)
    }));
    let result = match result {
        Ok(result) => result,
        Err(panic) => {
            work.producer.cells_remaining = 0;
            work.producer.json_nodes_remaining = 0;
            write.io.nodes_remaining = 0;
            write.io.read_bytes_remaining = 0;
            write.io.write_bytes_remaining = 0;
            write.key_work.bytes_remaining = 0;
            drop(child);
            std::panic::resume_unwind(panic)
        }
    };
    let after = [
        work.producer.cells_remaining,
        work.producer.json_nodes_remaining,
        write.io.nodes_remaining,
        write.io.read_bytes_remaining,
        write.io.write_bytes_remaining,
        write.key_work.bytes_remaining,
    ];
    let mut restored = before;
    let mut sums = [0u64; 2];
    let mut broken = false;
    for i in 0..6 {
        if let Some(used) = cap[i].checked_sub(after[i]) {
            restored[i] = before[i] - used;
            let j = usize::from(i >= 3);
            if let Some(sum) = sums[j].checked_add(used) {
                sums[j] = sum
            } else {
                broken = true
            }
        } else {
            restored[i] = 0;
            broken = true
        }
    }
    work.producer.cells_remaining = restored[0];
    work.producer.json_nodes_remaining = restored[1];
    write.io.nodes_remaining = restored[2];
    write.io.read_bytes_remaining = restored[3];
    write.io.write_bytes_remaining = restored[4];
    write.key_work.bytes_remaining = restored[5];
    let paid = child.consume(sums[0], sums[1]).map_err(formal);
    child.finish();
    match result {
        Err(error) => Err(error),
        Ok(value) => {
            paid?;
            if broken {
                return Err(invalid());
            }
            if stop() {
                return Err(ValueTreeError::Cancelled);
            }
            Ok(value)
        }
    }
}

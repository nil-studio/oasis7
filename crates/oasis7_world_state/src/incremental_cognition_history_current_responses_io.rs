//! One continuous Common child over the concrete Responses codec/index IO.
use super::*;
pub(super) fn metered<M, R, E, C, W, D, T>(
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    work: &mut CurrentCognitionResponsesWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    operation: impl FnOnce(
        &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        &mut MemberRecordWriteContext<'_, W, D>,
        &mut CurrentCognitionResponsesWorkV1<'_>,
        &mut dyn FnMut() -> bool,
    ) -> Result<T, MemberRecordError>,
) -> Result<T, MemberRecordError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    D: FnMut() -> bool,
{
    if read.values.class != MemoryClass::Ordinary
        || !read.values.memory.same_pool(write.memory)
        || !write.memory.same_pool(common.memory())
    {
        return Err(bad().into());
    }
    macro_rules! snapshot {
        () => {
            [
                work.indexes.cells_remaining,
                work.cbor.nodes_remaining,
                work.cbor.source_reads_remaining,
                work.processing.steps_remaining,
                read.values.io.nodes_remaining,
                write.io.nodes_remaining,
                work.cbor.bytes_remaining,
                work.cbor.key_bytes_remaining,
                work.processing.bytes_remaining,
                read.values.io.read_bytes_remaining,
                read.key_work.bytes_remaining,
                write.io.read_bytes_remaining,
                write.io.write_bytes_remaining,
                write.key_work.bytes_remaining,
            ]
        };
    }
    macro_rules! assign {
        ($a:expr) => {{
            let a = $a;
            work.indexes.cells_remaining = a[0];
            work.cbor.nodes_remaining = a[1];
            work.cbor.source_reads_remaining = a[2];
            work.processing.steps_remaining = a[3];
            read.values.io.nodes_remaining = a[4];
            write.io.nodes_remaining = a[5];
            work.cbor.bytes_remaining = a[6];
            work.cbor.key_bytes_remaining = a[7];
            work.processing.bytes_remaining = a[8];
            read.values.io.read_bytes_remaining = a[9];
            read.key_work.bytes_remaining = a[10];
            write.io.read_bytes_remaining = a[11];
            write.io.write_bytes_remaining = a[12];
            write.key_work.bytes_remaining = a[13];
        }};
    }
    let original = snapshot!();
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child().map_err(formal)?;
    let mut admitted = original;
    for (i, lane) in admitted.iter_mut().enumerate() {
        *lane = (*lane).min(if i < 6 {
            child.remaining_steps() / 6
        } else {
            child.remaining_read_bytes() / 8
        });
    }
    assign!(admitted);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if stop() {
            return Err(ValueTreeError::Cancelled.into());
        }
        operation(read, write, work, stop)
    }));
    let result = match result {
        Ok(r) => r,
        Err(p) => {
            assign!([0; 14]);
            drop(child);
            std::panic::resume_unwind(p)
        }
    };
    let after = snapshot!();
    let mut restored = original;
    let mut steps = 0u64;
    let mut bytes = 0u64;
    for i in 0..14 {
        let Some(d) = admitted[i].checked_sub(after[i]) else {
            assign!([0; 14]);
            drop(child);
            return result.and(Err(bad().into()));
        };
        restored[i] = original[i] - d;
        if i < 6 {
            steps = steps.checked_add(d).ok_or_else(bad)?;
        } else {
            bytes = bytes.checked_add(d).ok_or_else(bad)?;
        }
    }
    assign!(restored);
    let paid = child.consume(steps, bytes).map_err(formal);
    child.finish();
    let value = result?;
    paid?;
    if stop() {
        return Err(ValueTreeError::Cancelled.into());
    }
    read.check()?;
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(value)
}

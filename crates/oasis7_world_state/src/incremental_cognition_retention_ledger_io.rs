//! Private single-bank bridge for the original fixed JSON kernels. Read/write
//! source callbacks remain data IO; this adapter cannot choose a business rule.
use super::*;
fn snapshot<M, R, E, C, W, D>(
    read: &WorldFieldReadContextV1<'_, M, R, E, C>,
    write: Option<&MemberRecordWriteContext<'_, W, D>>,
    w: &CognitionRetentionWorkV1,
) -> [u64; 16] {
    [
        w.json.cells_remaining,
        w.json.json_nodes_remaining,
        w.producer.cells_remaining,
        w.producer.json_nodes_remaining,
        w.changed.json.cells_remaining,
        w.changed.json.json_nodes_remaining,
        w.changed.paths_remaining,
        w.processing.steps_remaining,
        read.values.io.nodes_remaining,
        write.map_or(0, |x| x.io.nodes_remaining),
        w.processing.bytes_remaining,
        read.values.io.read_bytes_remaining,
        read.key_work.bytes_remaining,
        write.map_or(0, |x| x.io.read_bytes_remaining),
        write.map_or(0, |x| x.io.write_bytes_remaining),
        write.map_or(0, |x| x.key_work.bytes_remaining),
    ]
}
fn assign<M, R, E, C, W, D>(
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: Option<&mut MemberRecordWriteContext<'_, W, D>>,
    w: &mut CognitionRetentionWorkV1,
    x: [u64; 16],
) {
    w.json.cells_remaining = x[0];
    w.json.json_nodes_remaining = x[1];
    w.producer.cells_remaining = x[2];
    w.producer.json_nodes_remaining = x[3];
    w.changed.json.cells_remaining = x[4];
    w.changed.json.json_nodes_remaining = x[5];
    w.changed.paths_remaining = x[6];
    w.processing.steps_remaining = x[7];
    read.values.io.nodes_remaining = x[8];
    w.processing.bytes_remaining = x[10];
    read.values.io.read_bytes_remaining = x[11];
    read.key_work.bytes_remaining = x[12];
    if let Some(write) = write {
        write.io.nodes_remaining = x[9];
        write.io.read_bytes_remaining = x[13];
        write.io.write_bytes_remaining = x[14];
        write.key_work.bytes_remaining = x[15]
    }
}
pub(super) fn run<M, R, E, C, W, D, T>(
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    mut write: Option<&mut MemberRecordWriteContext<'_, W, D>>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    operation: impl FnOnce(
        &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        Option<&mut MemberRecordWriteContext<'_, W, D>>,
        &mut CognitionRetentionWorkV1,
        &mut dyn FnMut() -> bool,
    ) -> Result<T, ValueTreeError>,
) -> Result<T, ValueTreeError> {
    let (_, meter, stop) = common.parts();
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    let child = meter.begin_child().map_err(formal)?;
    let before = snapshot(read, write.as_deref(), work);
    let mut cap = before;
    for value in &mut cap[..10] {
        *value = (*value).min(child.remaining_steps() / 10)
    }
    for value in &mut cap[10..] {
        *value = (*value).min(child.remaining_read_bytes() / 6)
    }
    assign(read, write.as_deref_mut(), work, cap);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        operation(read, write.as_deref_mut(), work, &mut *stop)
    }));
    let result = match result {
        Ok(result) => result,
        Err(panic) => {
            assign(read, write.as_deref_mut(), work, [0; 16]);
            drop(child);
            std::panic::resume_unwind(panic)
        }
    };
    let after = snapshot(read, write.as_deref(), work);
    let mut restore = before;
    let mut totals = [0u64; 2];
    let mut broken = false;
    for i in 0..16 {
        if let Some(used) = cap[i].checked_sub(after[i]) {
            restore[i] = before[i] - used;
            let lane = usize::from(i >= 10);
            if let Some(n) = totals[lane].checked_add(used) {
                totals[lane] = n
            } else {
                broken = true
            }
        } else {
            restore[i] = 0;
            broken = true
        }
    }
    assign(read, write, work, restore);
    let paid = child.consume(totals[0], totals[1]).map_err(formal);
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

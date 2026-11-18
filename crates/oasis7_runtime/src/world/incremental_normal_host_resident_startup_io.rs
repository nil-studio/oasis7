//! Single exclusive Common child for the original two startup producers.
use super::*;
pub(super) fn selected<M, R, E, C>(
    parent: &BoundNormalTickParentV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut NormalHostStartupWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<
    (
        ReservedValue<AcceptedWorldManifestHashV1>,
        ReservedValue<ParentInventoryMaterialScopeV1>,
    ),
    BodyMutationError,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
{
    if !read.values.memory.same_pool(common.memory()) || read.values.class != MemoryClass::Ordinary
    {
        return Err(bad());
    }
    common.charge(1, 4096).map_err(formal)?;
    let _phase = common
        .memory()
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    macro_rules! apply {
     (clamp_b; $($args:tt)*) => { clamp_b!($($args)*) };
     (clamp_s; $($args:tt)*) => { clamp_s!($($args)*) };
     (restore_b; $($args:tt)*) => { restore_b!($($args)*) };
     (restore_s; $($args:tt)*) => { restore_s!($($args)*) };
     (snapshot; $($args:tt)*) => { snapshot!($($args)*) };
 }
    macro_rules! steps{($m:ident)=>{apply!($m;read.values.io.nodes_remaining,work.manifest.body_scan.nodes_remaining,work.manifest.body_scan.leaves_remaining,work.manifest.fields.cells_remaining,work.manifest.fields.json.cells_remaining,work.manifest.fields.json.json_nodes_remaining,work.manifest.json.cells_remaining,work.manifest.json.json_nodes_remaining,work.material_scan.nodes_remaining,work.material_scan.leaves_remaining,work.inventory.records_remaining,work.inventory.index_steps_remaining )};}
    macro_rules! bytes{($m:ident)=>{apply!($m;read.values.io.read_bytes_remaining,read.values.io.write_bytes_remaining,read.key_work.bytes_remaining,work.manifest.body_scan.read_bytes_remaining,work.manifest.hash_bytes_remaining,work.material_scan.read_bytes_remaining,work.material.comparison_bytes_remaining )};}
    macro_rules! snapshot{($($lane:expr),*)=>{[$($lane),*]};}
    let original_s = steps!(snapshot);
    let original_b = bytes!(snapshot);
    let original_n = work.material.pending_entries_remaining;
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child().map_err(formal)?;
    let sc = child.remaining_steps() / 13;
    let bc = child.remaining_read_bytes() / 7;
    macro_rules! clamp_s{($($lane:expr),*)=>{$($lane=$lane.min(sc);)*};}
    macro_rules! clamp_b{($($lane:expr),*)=>{$($lane=$lane.min(bc);)*};}
    steps!(clamp_s);
    bytes!(clamp_b);
    work.material.pending_entries_remaining = work
        .material
        .pending_entries_remaining
        .min(usize::try_from(sc).unwrap_or(usize::MAX));
    let before_s = steps!(snapshot);
    let before_b = bytes!(snapshot);
    let before_n = work.material.pending_entries_remaining;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut cancel = || stop() || (read.cancelled)();
        let mut selected = WorldFieldReadContextV1 {
            root: parent.refs().roots.state_root(StateNamespaceV1::Artifacts),
            state: read.state,
            values: ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut cancel,
        };
        let manifest = recover_world_manifest_hash_v1(
            parent.refs().runtime,
            parent.refs().head,
            parent.refs().roots,
            parent.refs().storage.current().runtime().config(),
            &mut selected,
            work.manifest,
        )
        .map_err(original)?;
        selected.root = parent.refs().roots.state_root(StateNamespaceV1::Business);
        let materials = build_parent_inventory_material_scope_v1(
            parent.refs().storage.current().native().inventory(),
            parent.refs().roots,
            &mut selected,
            work.material_scan,
            work.inventory,
            work.material,
        )?;
        Ok::<_, BodyMutationError>((manifest, materials))
    }));
    let after_s = steps!(snapshot);
    let after_b = bytes!(snapshot);
    let after_n = work.material.pending_entries_remaining;
    let debit = |b: &[u64], a: &[u64]| {
        b.iter()
            .zip(a)
            .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
    };
    let accounting = debit(&before_s, &after_s)
        .and_then(|n| n.checked_add(u64::try_from(before_n.checked_sub(after_n)?).ok()?))
        .zip(debit(&before_b, &after_b));
    // Restore only the caller's unadmitted balances. Real child consumption is
    // never refunded, on error or panic included.
    let mut i = 0;
    macro_rules! restore_s{($($lane:expr),*)=>{$({$lane=original_s[i].saturating_sub(before_s[i].saturating_sub(after_s[i]));i+=1;})*};}
    steps!(restore_s);
    let _ = i;
    let mut i = 0;
    macro_rules! restore_b{($($lane:expr),*)=>{$({$lane=original_b[i].saturating_sub(before_b[i].saturating_sub(after_b[i]));i+=1;})*};}
    bytes!(restore_b);
    let _ = i;
    work.material.pending_entries_remaining =
        original_n.saturating_sub(before_n.saturating_sub(after_n));
    let paid = match accounting {
        Some((s, b)) => child.consume(s, b).map_err(formal),
        None => Err(bad()),
    };
    let result = settle(child, outcome, paid)?;
    common.consume_steps(0).map_err(formal)?;
    Ok(result)
}

/// A normal returned error still settles its actual reads. A panic or a reader
/// whose accounting cannot be closed abandons the original bank instead.
fn settle<T>(
    child: oasis7_formal_protocol::controlled_authority::formal::bounded_context::ChildLeaseV1<'_>,
    outcome: std::thread::Result<Result<T, BodyMutationError>>,
    paid: Result<(), BodyMutationError>,
) -> Result<T, BodyMutationError> {
    match outcome {
        Err(payload) => {
            drop(child);
            std::panic::resume_unwind(payload)
        }
        Ok(result) => {
            if paid.is_ok() {
                child.finish();
            } else {
                drop(child);
            }
            let result = result?;
            paid?;
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;

    #[test]
    fn accounted_producer_failure_keeps_its_reads_and_unlocks_the_same_bank() {
        let bank = FormalBoundedWorkV1::new(1_000, 10_000).unwrap();
        let child = bank.begin_child().unwrap();
        let paid = child.consume(3, 7).map_err(formal);
        let result = settle::<()>(
            child,
            Ok(Err(BodyMutationError::Invalid(
                "first startup read failure",
            ))),
            paid,
        );
        assert!(matches!(
            result,
            Err(BodyMutationError::Invalid("first startup read failure"))
        ));
        assert_eq!(
            (bank.remaining_steps(), bank.remaining_read_bytes()),
            (997, 9_993)
        );
        assert!(!bank.is_child_active());
        bank.charge(2, 5, &mut || false).unwrap();
        assert_eq!(
            (bank.remaining_steps(), bank.remaining_read_bytes()),
            (995, 9_988)
        );
    }

    #[test]
    fn unaccounted_startup_burns_common_without_replacing_the_first_error() {
        for producer_failed in [false, true] {
            let bank = FormalBoundedWorkV1::new(1_000, 10_000).unwrap();
            let child = bank.begin_child().unwrap();
            let producer = if producer_failed {
                Err(BodyMutationError::Invalid("first startup read failure"))
            } else {
                Ok(())
            };
            let result = settle(
                child,
                Ok(producer),
                Err(BodyMutationError::Invalid("startup counter grew")),
            );
            let expected = if producer_failed {
                "first startup read failure"
            } else {
                "startup counter grew"
            };
            assert!(
                matches!(result, Err(BodyMutationError::Invalid(message)) if message == expected)
            );
            assert_eq!(
                (bank.remaining_steps(), bank.remaining_read_bytes()),
                (0, 0)
            );
            assert!(!bank.is_child_active());
        }
    }

    #[test]
    fn startup_panic_burns_common_even_when_scalar_accounting_succeeds() {
        for accounting_failed in [false, true] {
            let bank = FormalBoundedWorkV1::new(1_000, 10_000).unwrap();
            let child = bank.begin_child().unwrap();
            let paid = if accounting_failed {
                Err(BodyMutationError::Invalid("startup counter grew"))
            } else {
                child.consume(3, 7).map_err(formal)
            };
            let producer =
                std::panic::catch_unwind(|| std::panic::panic_any("original startup panic"));
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                settle::<()>(child, producer, paid)
            }));
            let payload = outcome.unwrap_err();
            assert_eq!(
                payload.downcast_ref::<&str>(),
                Some(&"original startup panic")
            );
            assert_eq!(
                (bank.remaining_steps(), bank.remaining_read_bytes()),
                (0, 0)
            );
            assert!(!bank.is_child_active());
        }
    }
}

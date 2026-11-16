//! One exclusive Common child over both real original IO loans. No callback
//! receives Common, and no scalar allowance is minted or refunded.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError, bounded_context::FormalBoundedContextV1,
};
#[derive(Debug)]
pub(in crate::iws_host_runtime) enum PairedNormalOriginalMeterErrorV1 {
    Context(FormalError),
    Store(PageStoreError),
}
fn get(io: &ObjectIo<'_>) -> [u64; 7] {
    [
        io.physical.reads_remaining,
        io.physical.writes_remaining,
        io.logical.nodes_remaining,
        io.physical.read_bytes_remaining,
        io.physical.write_bytes_remaining,
        io.logical.read_bytes_remaining,
        io.logical.write_bytes_remaining,
    ]
}
fn set(io: &mut ObjectIo<'_>, v: [u64; 7]) {
    io.physical.reads_remaining = v[0];
    io.physical.writes_remaining = v[1];
    io.logical.nodes_remaining = v[2];
    io.physical.read_bytes_remaining = v[3];
    io.physical.write_bytes_remaining = v[4];
    io.logical.read_bytes_remaining = v[5];
    io.logical.write_bytes_remaining = v[6];
}
impl HostNormalOriginalsV1<'_, '_, '_> {
    pub(in crate::iws_host_runtime) fn with_metered_paired_reader<T>(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
        run: impl FnOnce(&mut HostNormalOriginalReaderV1<'_, '_, '_, '_>) -> Result<T, PageStoreError>,
    ) -> Result<T, PairedNormalOriginalMeterErrorV1> {
        self.with_metered_reader_kernel(true, common, run)
    }
    pub(in crate::iws_host_runtime) fn with_metered_actual_reader<T>(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
        run: impl FnOnce(&mut HostNormalOriginalReaderV1<'_, '_, '_, '_>) -> Result<T, PageStoreError>,
    ) -> Result<T, PairedNormalOriginalMeterErrorV1> {
        self.with_metered_reader_kernel(false, common, run)
    }
    fn with_metered_reader_kernel<T>(
        &self,
        require_pair: bool,
        common: &mut FormalBoundedContextV1<'_>,
        run: impl FnOnce(&mut HostNormalOriginalReaderV1<'_, '_, '_, '_>) -> Result<T, PageStoreError>,
    ) -> Result<T, PairedNormalOriginalMeterErrorV1> {
        use PairedNormalOriginalMeterErrorV1::{Context, Store};
        common
            .charge(56, (8 * std::mem::size_of::<[u64; 7]>()) as u64)
            .map_err(Context)?;
        let frame_fee = common
            .memory()
            .reserve(9 * std::mem::size_of::<[u64; 7]>(), MemoryClass::Ordinary)
            .map_err(|e| Store(PageStoreError::Memory(e)))?;
        let replica = self.replica.as_ref().map(|(_, io)| io);
        if require_pair && replica.is_none() {
            return Err(Store(PageStoreError::Corrupt(
                "normal metered originals require pair",
            )));
        }
        let mut primary = self.io.try_borrow_mut().map_err(|_| Store(reentrant()))?;
        let mut second = replica
            .map(|io| io.try_borrow_mut())
            .transpose()
            .map_err(|_| Store(reentrant()))?;
        let before = [
            get(&primary),
            second.as_ref().map(|io| get(io)).unwrap_or([0; 7]),
        ];
        let lease = common.work().begin_child().map_err(Context)?;
        // Three step lanes and four byte lanes at each endpoint. This is the
        // same conservative lane partition as the Root external IO mapper.
        let steps = lease.remaining_steps() / if replica.is_some() { 6 } else { 3 };
        let bytes = lease.remaining_read_bytes() / if replica.is_some() { 8 } else { 4 };
        let capped = before
            .map(|row| std::array::from_fn(|i| row[i].min(if i < 3 { steps } else { bytes })));
        set(&mut primary, capped[0]);
        if let Some(second) = second.as_mut() {
            set(second, capped[1]);
        }
        drop(second);
        drop(primary); // never retain RefCell guards across IO
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut reader = self.reader();
            run(&mut reader)
        }));
        let reconcile = (|| {
            let mut primary = self.io.try_borrow_mut().map_err(|_| reentrant())?;
            let mut second = replica
                .map(|io| io.try_borrow_mut())
                .transpose()
                .map_err(|_| reentrant())?;
            if result.is_err() {
                set(&mut primary, [0; 7]);
                if let Some(second) = second.as_mut() {
                    set(second, [0; 7]);
                }
                return Ok(None);
            }
            let after = [
                get(&primary),
                second.as_ref().map(|io| get(io)).unwrap_or([0; 7]),
            ];
            let mut restored = before;
            let (mut used_steps, mut used_bytes) = (0u64, 0u64);
            for endpoint in 0..2 {
                for lane in 0..7 {
                    let Some(used) = capped[endpoint][lane].checked_sub(after[endpoint][lane])
                    else {
                        set(&mut primary, [0; 7]);
                        if let Some(second) = second.as_mut() {
                            set(second, [0; 7]);
                        }
                        return Err(PageStoreError::Corrupt("normal original lane increased"));
                    };
                    restored[endpoint][lane] = before[endpoint][lane]
                        .checked_sub(used)
                        .ok_or(PageStoreError::Corrupt("normal original restoration"))?;
                    let total = if lane < 3 {
                        &mut used_steps
                    } else {
                        &mut used_bytes
                    };
                    *total = total
                        .checked_add(used)
                        .ok_or(PageStoreError::Corrupt("normal original debit overflow"))?;
                }
            }
            set(&mut primary, restored[0]);
            if let Some(second) = second.as_mut() {
                set(second, restored[1]);
            }
            Ok(Some((used_steps, used_bytes)))
        })();
        let debit = match reconcile {
            Ok(Some((s, b))) => lease.consume(s, b).map_err(Context),
            Ok(None) => Err(Store(PageStoreError::Corrupt("normal original IO panic"))),
            Err(error) => {
                if let Ok(mut io) = self.io.try_borrow_mut() {
                    set(&mut io, [0; 7]);
                }
                if let Some(replica) = replica {
                    if let Ok(mut io) = replica.try_borrow_mut() {
                        set(&mut io, [0; 7]);
                    }
                }
                let _ = lease.consume(u64::MAX, u64::MAX);
                Err(Store(error))
            }
        };
        match result {
            Err(payload) => {
                drop(lease);
                drop(frame_fee);
                std::panic::resume_unwind(payload)
            }
            Ok(Err(error)) => {
                lease.finish();
                Err(Store(error))
            }
            Ok(Ok(value)) => {
                lease.finish();
                debit?;
                Ok(value)
            }
        }
    }
}

//! Original HOT bytes for the response, after the original guard/reconcile.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

impl Store {
    pub(in crate::controlled_authority::formal::endpoint) fn checkpoint_bytes_bounded(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, FormalError> {
        let length = self
            .path
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .checked_add(HOT.len() + 1)
            .ok_or_else(|| invalid("formal checkpoint path layout"))?;
        let capacity = length
            .checked_mul(2)
            .ok_or_else(|| invalid("formal checkpoint path layout"))?;
        let allocation = capacity
            .checked_add(std::mem::size_of::<PathBuf>())
            .ok_or_else(|| invalid("formal checkpoint path layout"))?;
        ctx.charge(1, length as u64)?;
        let _path_fee = ctx
            .memory()
            .reserve(allocation, MemoryClass::Ordinary)
            .map_err(|_| invalid("formal checkpoint path admission"))?;
        let mut path = PathBuf::with_capacity(length);
        path.push(&self.path);
        path.push(HOT);
        if path.capacity() > capacity {
            return Err(invalid("formal checkpoint path capacity"));
        }
        // HOT retains its original regular/64 MiB policy, without generation's
        // parent/UID/write-mode restrictions. Recheck the actual opened file.
        regular(&path)?;
        let mut file = File::open(&path).map_err(io)?;
        let metadata = file.metadata().map_err(io)?;
        if !metadata.is_file() || metadata.len() > LIMIT as u64 {
            return Err(invalid("formal storage regular file/size"));
        }
        let length = usize::try_from(metadata.len())
            .map_err(|_| invalid("formal checkpoint byte layout"))?;
        let (memory, work, stop) = ctx.parts();
        let lease = work.begin_child()?;
        let before = (lease.remaining_steps(), lease.remaining_read_bytes());
        let mut scalar = super::super::super::physical::GenerationSelectedReadWorkV1 {
            remaining_steps: before.0,
            remaining_read_bytes: before.1,
        };
        let result = super::super::super::physical::read_opened_exact_bounded_v1(
            &mut file,
            length,
            memory,
            &mut scalar,
            &mut || stop(),
        );
        let delta = before
            .0
            .checked_sub(scalar.remaining_steps)
            .zip(before.1.checked_sub(scalar.remaining_read_bytes));
        let debit = match delta {
            Some((steps, bytes)) => lease.consume(steps, bytes),
            None => {
                drop(lease);
                return result
                    .and_then(|_| Err(invalid("bounded archive child counters increased")));
            }
        };
        lease.finish();
        let bytes = match result {
            Err(error) => return Err(error),
            Ok(bytes) => {
                debit?;
                bytes
            }
        };
        let after = file.metadata().map_err(io)?;
        if !after.is_file() || after.len() != metadata.len() {
            return Err(invalid("formal checkpoint changed while reading"));
        }
        ctx.consume_steps(1)?;
        Ok(bytes)
    }
}

//! Native checkpoint replacement; caller must hold its genuine transaction owner.
use super::*;
struct TempCleanup<'a> {
    path: &'a Path,
    active: bool,
}
impl Drop for TempCleanup<'_> {
    fn drop(&mut self) {
        if self.active {
            let _ = fs::remove_file(self.path);
        }
    }
}
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
impl Store {
    pub(super) fn atomic_hot_bounded(
        &self,
        bytes: &[u8],
        memory: &MemoryBudget,
        work: &super::super::FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        if bytes.len() > LIMIT {
            return Err(invalid("formal storage byte limit"));
        }
        work.charge(1, 0, stop)?;
        // Two PathBuf payloads plus escaped IO diagnostics. Temp suffix contains
        // two u64-width decimal numbers, separators and a fixed prefix.
        let root = self.path.as_os_str().as_encoded_bytes().len();
        let path = root
            .checked_add(96)
            .ok_or_else(|| invalid("bounded atomic path layout"))?;
        let layout = path
            .checked_mul(32)
            .and_then(|n| n.checked_add(2 * std::mem::size_of::<PathBuf>() + 4096))
            .ok_or_else(|| invalid("bounded atomic path layout"))?;
        let _fee = memory
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(|_| invalid("bounded atomic path admission"))?;
        work.charge(
            1,
            path.checked_mul(2)
                .ok_or_else(|| invalid("bounded atomic path layout"))? as u64,
            stop,
        )?;
        // Prepay one bounded best-effort cleanup attempt before creation.
        work.charge(1, 0, stop)?;
        let temp = self.path.join(format!(
            ".formal-temp-{}-{}",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let target = self.path.join(HOT);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        work.charge(1, 0, stop)?;
        let mut file = options.open(&temp).map_err(io)?;
        let mut cleanup = TempCleanup {
            path: &temp,
            active: true,
        };
        let mut at = 0;
        while at < bytes.len() {
            let end = at.saturating_add(8192).min(bytes.len());
            work.charge(1, (end - at) as u64, stop)?;
            match file.write(&bytes[at..end]) {
                Ok(0) => return Err(io(std::io::Error::from(std::io::ErrorKind::WriteZero))),
                Ok(n) => at += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(io(e)),
            }
        }
        work.charge(1, 0, stop)?;
        file.sync_all().map_err(io)?;
        work.charge(1, 0, stop)?;
        fs::rename(&temp, &target).map_err(io)?;
        cleanup.active = false;
        // Cancellation after rename reports an unknown durability outcome to
        // the caller; it must retain provider pending and reconcile on retry.
        work.charge(1, 0, stop)?;
        let directory = File::open(&self.path).map_err(io)?;
        work.charge(1, 0, stop)?;
        directory.sync_all().map_err(io)
    }
}

//! One bounded physical page per owner tick, with quiet-tail and growth debt.
use super::*;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
#[derive(Default)]
pub(in super::super) struct MaintenanceState {
    pub(super) generation: super::super::super::physical::GenerationSession,
    pub(super) inventory_session: Option<String>,
    observed: Option<String>,
    observed_count: u64,
    completed_count: u64,
    dirty_since: Option<Instant>,
    last_change: Option<Instant>,
    last_finished: Option<Instant>,
    next_page: Option<Instant>,
    #[cfg(test)]
    short_quiet: bool,
}
fn maintenance_io(error: impl std::fmt::Display) -> FormalError {
    FormalError::Io(error.to_string())
}
fn observation(root: &Path) -> Result<String, FormalError> {
    let mut entries = Vec::new();
    for name in [
        "",
        ".frames-v1",
        ".representation-v1",
        ".physical-staging-v1",
        ".business-pack-staging-v1",
    ] {
        match fs::symlink_metadata(root.join(name)) {
            Ok(metadata) => {
                super::super::super::physical::paths::metadata(&metadata, true)?;
                let modified = metadata
                    .modified()
                    .map_err(maintenance_io)?
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(maintenance_io)?
                    .as_nanos();
                entries.push((name, metadata.len(), modified));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                entries.push((name, 0, 0))
            }
            Err(error) => return Err(maintenance_io(error)),
        }
    }
    Ok(
        blake3::hash(&serde_cbor::to_vec(&entries).map_err(maintenance_io)?)
            .to_hex()
            .to_string(),
    )
}
fn has_debris(root: &Path) -> Result<bool, FormalError> {
    for name in [
        ".frames-v1",
        ".representation-v1",
        ".physical-staging-v1",
        ".business-pack-staging-v1",
    ] {
        match fs::read_dir(root.join(name)) {
            Ok(mut entries) => {
                if let Some(entry) = entries.next() {
                    entry.map_err(maintenance_io)?;
                    return Ok(true);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(maintenance_io(error)),
        }
    }
    Ok(false)
}
impl FileFormalEndpoint {
    /// Local owner maintenance only. It creates no new protocol permission and
    /// never changes an already-written RPC response or protected logical head.
    #[cfg(test)]
    pub(crate) fn short_maintenance_quiet_for_test(&mut self) {
        self.physical_maintenance.short_quiet = true;
    }
    pub(crate) fn maintenance_tick(&mut self, now: Instant) -> Result<bool, FormalError> {
        let root = self.store.path.join("formal-objects-v1");
        if !super::super::super::physical::enabled(&root)? {
            return Ok(false);
        }
        let state = &mut self.physical_maintenance;
        if state.next_page.is_some_and(|next| now < next) {
            return Ok(false);
        }
        state.next_page = Some(now + Duration::from_secs(1));
        let count = super::super::super::physical::roots::count(&root)?;
        let observed = observation(&root)?;
        let building = root
            .join(".generation-build-v1.cbor")
            .try_exists()
            .map_err(maintenance_io)?;
        let selected = root
            .join(".generation-current-v1")
            .try_exists()
            .map_err(maintenance_io)?;
        if state.observed.is_none() {
            state.completed_count = if selected { count } else { 0 };
            if building || (!selected && count > 0) || has_debris(&root)? {
                state.dirty_since = Some(now);
                state.last_change = Some(now);
            }
        } else if state.observed_count != count
            || (!building && state.observed.as_ref() != Some(&observed))
        {
            state.dirty_since.get_or_insert(now);
            state.last_change = Some(now);
        }
        state.observed = Some(observed);
        state.observed_count = count;
        let growth =
            count.saturating_sub(state.completed_count) >= 1024.max(state.completed_count / 4);
        let quiet_period = Duration::from_secs(30);
        #[cfg(test)]
        let quiet_period = if state.short_quiet {
            Duration::from_millis(10)
        } else {
            quiet_period
        };
        let quiet = state
            .last_change
            .is_some_and(|last| now.saturating_duration_since(last) >= quiet_period);
        let cooled = state
            .last_finished
            .is_none_or(|last| now.saturating_duration_since(last) >= Duration::from_secs(300));
        if !building
            && !(state.dirty_since.is_some()
                && (growth || (quiet && ((!selected && count > 0) || cooled))))
        {
            return Ok(false);
        }
        // A pending read pin is expected backpressure, not a maintenance error.
        match super::super::super::physical::pin(&root, true) {
            Ok(pin) => drop(pin),
            Err(FormalError::Conflict(_)) => return Ok(false),
            Err(error) => return Err(error),
        }
        match self.advance_automatic_physical_generation() {
            Ok(progress) => {
                if !progress.building {
                    let state = &mut self.physical_maintenance;
                    state.completed_count = super::super::super::physical::roots::count(&root)?;
                    state.observed_count = state.completed_count;
                    state.observed = Some(observation(&root)?);
                    state.last_finished = Some(now);
                    state.dirty_since = None;
                    state.last_change = None;
                }
                Ok(true)
            }
            Err(FormalError::Conflict(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }
}

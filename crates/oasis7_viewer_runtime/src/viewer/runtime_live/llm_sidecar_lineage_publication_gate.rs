//! Test-only gate at the real encoded temp-file publication boundary.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};
#[derive(Default)]
struct State {
    entered: Option<PathBuf>,
    released: bool,
    claimed: bool,
    published: bool,
}
#[derive(Default)]
struct Gate {
    state: Mutex<State>,
    changed: Condvar,
    before_write: bool,
    prepare_only: bool,
    fresh_original_only: bool,
}
thread_local! { static IN_PREPARE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(in crate::viewer::runtime_live) struct PrepareScope(bool);
impl Drop for PrepareScope {
    fn drop(&mut self) {
        IN_PREPARE.with(|flag| flag.set(self.0));
    }
}
pub(in crate::viewer::runtime_live) fn in_prepare() -> bool {
    IN_PREPARE.with(std::cell::Cell::get)
}
pub(in crate::viewer::runtime_live) fn prepare_scope() -> PrepareScope {
    PrepareScope(IN_PREPARE.with(|flag| flag.replace(true)))
}
fn registry() -> &'static Mutex<BTreeMap<PathBuf, Arc<Gate>>> {
    static GATES: OnceLock<Mutex<BTreeMap<PathBuf, Arc<Gate>>>> = OnceLock::new();
    GATES.get_or_init(Mutex::default)
}
pub(super) fn wait_at(
    path: &Path,
    temporary: &Path,
    before_write: bool,
    fresh_original: bool,
) -> Result<(), String> {
    let gate = registry().lock().unwrap().get(path).cloned();
    let Some(gate) = gate else { return Ok(()) };
    if gate.prepare_only && !IN_PREPARE.with(std::cell::Cell::get) {
        return Ok(());
    }
    if gate.fresh_original_only && !fresh_original {
        return Ok(());
    }
    if gate.before_write != before_write {
        return Ok(());
    }
    let mut state = gate.state.lock().unwrap();
    if state.claimed {
        return Ok(());
    }
    state.claimed = true;
    state.entered = Some(temporary.to_owned());
    gate.changed.notify_all();
    let (state, timeout) = gate
        .changed
        .wait_timeout_while(state, Duration::from_secs(10), |s| !s.released)
        .unwrap();
    if timeout.timed_out() && !state.released {
        return Err("test checkpoint publication gate timed out".into());
    }
    Ok(())
}
pub(super) fn after_rename(path: &Path, temporary: &Path) {
    if let Some(gate) = registry().lock().unwrap().get(path).cloned() {
        let mut state = gate.state.lock().unwrap();
        if state.entered.as_deref() == Some(temporary) {
            state.published = true;
        }
    }
}
impl crate::viewer::ViewerRuntimeLiveServer {
    pub fn test_arm_lineage_publication_gate(path: &Path) {
        assert!(
            registry()
                .lock()
                .unwrap()
                .insert(
                    path.to_owned(),
                    Arc::new(Gate {
                        prepare_only: true,
                        ..Gate::default()
                    })
                )
                .is_none()
        );
    }
    pub fn test_arm_fresh_lineage_publication_gate(path: &Path) {
        let gate = Gate {
            prepare_only: true,
            fresh_original_only: true,
            ..Gate::default()
        };
        assert!(
            registry()
                .lock()
                .unwrap()
                .insert(path.to_owned(), Arc::new(gate))
                .is_none()
        );
    }
    pub fn test_arm_lineage_encoded_gate(path: &Path) {
        let gate = Gate {
            before_write: true,
            ..Gate::default()
        };
        assert!(
            registry()
                .lock()
                .unwrap()
                .insert(path.to_owned(), Arc::new(gate))
                .is_none()
        );
    }
    pub fn test_wait_lineage_publication_gate(path: &Path, timeout: Duration) -> Option<PathBuf> {
        let gate = registry().lock().unwrap().get(path).cloned()?;
        let deadline = Instant::now() + timeout;
        let mut state = gate.state.lock().unwrap();
        while state.entered.is_none() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (next, _) = gate.changed.wait_timeout(state, remaining).unwrap();
            state = next;
        }
        state.entered.clone()
    }
    pub fn test_lineage_publication_gate_published(path: &Path) -> bool {
        registry()
            .lock()
            .unwrap()
            .get(path)
            .is_some_and(|gate| gate.state.lock().unwrap().published)
    }
    pub fn test_clear_lineage_publication_gate(path: &Path) {
        Self::test_release_lineage_publication_gate(path);
        registry().lock().unwrap().remove(path);
    }
    pub fn test_release_lineage_publication_gate(path: &Path) {
        if let Some(gate) = registry().lock().unwrap().get(path).cloned() {
            gate.state.lock().unwrap().released = true;
            gate.changed.notify_all();
        }
    }
}
#[cfg(test)]
#[path = "llm_sidecar_lineage_publication_tests.rs"]
mod tests;

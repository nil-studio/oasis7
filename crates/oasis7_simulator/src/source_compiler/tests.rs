use super::*;
use crate::GeoPos;
use crate::{Action, LocationProfile, RejectReason, WorldEventKind, WorldKernel};
use oasis7_agent_api::module_source::ModuleSourceCompileError;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Compiler {
    calls: Arc<AtomicUsize>,
    result: Result<Vec<u8>, ModuleSourceCompileError>,
}
impl ModuleSourceCompiler for Compiler {
    fn compile(
        &self,
        module_id: &str,
        package: &ModuleSourcePackage,
    ) -> Result<Vec<u8>, ModuleSourceCompileError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(module_id, "m.injected");
        assert_eq!(package.manifest_path, "Cargo.toml");
        assert_eq!(package.files["Cargo.toml"], b"source");
        self.result.clone()
    }
}
fn kernel() -> WorldKernel {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "origin".into(),
        name: "Origin".into(),
        pos: GeoPos::new(0, 0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent".into(),
        location_id: "origin".into(),
    });
    kernel.step_until_empty();
    kernel
}
fn action() -> Action {
    Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "agent".into(),
        module_id: "  m.injected  ".into(),
        manifest_path: " Cargo.toml ".into(),
        source_files: BTreeMap::from([("Cargo.toml".into(), b"source".to_vec())]),
    }
}
#[test]
fn injected_compiler_executes_once_and_replay_consumes_recorded_bytes_without_compiling() {
    let mut kernel = kernel();
    let base = kernel.kernel_snapshot();
    let calls = Arc::new(AtomicUsize::new(0));
    kernel.set_module_source_compiler(Arc::new(Compiler {
        calls: calls.clone(),
        result: Ok(b"compiled artifact".to_vec()),
    }));
    kernel.submit_action(action());
    assert_eq!(calls.load(Ordering::SeqCst), 0, "queueing must not compile");
    let event = kernel.step().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(event.id, base.next_event_id);
    let WorldEventKind::ModuleArtifactDeployed {
        wasm_bytes,
        bytes_len,
        module_id_hint,
        ..
    } = event.kind
    else {
        panic!("not deployed")
    };
    assert_eq!(wasm_bytes, b"compiled artifact");
    assert_eq!(bytes_len, wasm_bytes.len() as u64);
    assert_eq!(module_id_hint.as_deref(), Some("m.injected"));
    let replay_calls = Arc::new(AtomicUsize::new(0));
    let replayed = WorldKernel::replay_from_kernel_snapshot_with_source_compiler(
        base,
        kernel.kernel_journal_snapshot(),
        Arc::new(Compiler {
            calls: replay_calls.clone(),
            result: Err(ModuleSourceCompileError::Unavailable(
                "must not compile during replay".into(),
            )),
        }),
    )
    .unwrap();
    assert_eq!(replay_calls.load(Ordering::SeqCst), 0);
    assert_eq!(replayed.kernel_snapshot(), kernel.kernel_snapshot());
}
#[test]
fn injected_compiler_failure_preserves_error_classification_without_deploying() {
    let mut kernel = kernel();
    let expected_id = kernel.kernel_snapshot().next_event_id;
    let calls = Arc::new(AtomicUsize::new(0));
    kernel.set_module_source_compiler(Arc::new(Compiler {
        calls: calls.clone(),
        result: Err(ModuleSourceCompileError::ModuleChangeInvalid {
            reason: "tool refused".into(),
        }),
    }));
    kernel.submit_action(action());
    let event = kernel.step().unwrap();
    assert_eq!(event.id, expected_id);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(kernel.model().module_artifacts.is_empty());
    let WorldEventKind::ActionRejected {
        reason: RejectReason::RuleDenied { notes },
    } = event.kind
    else {
        panic!("not rejected")
    };
    assert_eq!(
        notes,
        vec![r#"compile module source rejected: ModuleChangeInvalid { reason: "tool refused" }"#]
    );
}

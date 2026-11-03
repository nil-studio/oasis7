use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1, incremental_execution::RootSetV1,
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{cell::Cell, path::Path};
pub(super) struct OrdinaryFixtureProofs {
    pub view: ReservedValue<AcceptedCognitionHistoryViewV1>,
    pub current: ReservedValue<CognitionHistoryCurrentReadmissionV1>,
    pub index: Option<ReservedValue<CognitionHistoryIndexQualificationV1>>,
}
pub(super) struct Frame<'a, 'quota> {
    pub selected: &'a ReservedValue<AcceptedCognitionHistoryViewV1>,
    pub index: Option<&'a ReservedValue<CognitionHistoryIndexQualificationV1>>,
    pub binding: &'a ReservedValue<CognitionHistoryCurrentBindingV1>,
    pub current: CognitionHistoryCurrentSourceV1<'a>,
    pub head: &'a VerifiedFormalCommitV1,
    pub roots: &'a RootSetV1,
    pub memory: &'a MemoryBudget,
    pub io: &'a mut ObjectIo<'quota>,
    pub source_path: &'a Path,
    pub ordinary_limit: usize,
    pub expected_json_slots: Option<usize>,
    pub ordinary_hold: Option<oasis7_proto::memory_budget::MemoryReservation>,
}
impl Frame<'_, '_> {
    fn source(&self) -> CognitionHistoryCurrentSourceV1<'_> {
        match &self.current {
            CognitionHistoryCurrentSourceV1::Enabled(proof) => {
                CognitionHistoryCurrentSourceV1::Enabled(proof)
            }
            CognitionHistoryCurrentSourceV1::Disabled(proof) => {
                CognitionHistoryCurrentSourceV1::Disabled(proof)
            }
        }
    }
}
struct Unavailable(PathBuf, PathBuf);
impl Unavailable {
    fn new(path: &Path) -> Self {
        let moved = path.with_extension("unavailable-for-readmission");
        std::fs::rename(path, &moved).unwrap();
        Self(path.to_path_buf(), moved)
    }
}
impl Drop for Unavailable {
    fn drop(&mut self) {
        std::fs::rename(&self.1, &self.0).unwrap();
    }
}
pub(super) fn run(mut frame: Frame<'_, '_>) -> OrdinaryFixtureProofs {
    // Harness fault outside the measured operation: even an accidental access
    // to the original store cannot fetch the old large JSON graph.
    let unavailable = Unavailable::new(frame.source_path);
    let before = (
        frame.io.physical.reads_remaining,
        frame.io.physical.read_bytes_remaining,
        frame.io.logical.nodes_remaining,
        frame.io.logical.read_bytes_remaining,
    );
    assert!(matches!(
        readmit_cognition_history_view_ordinary_v1(
            frame.selected,
            frame.head,
            frame.roots,
            frame.memory,
            || false,
        ),
        Err(CognitionHistorySelectionErrorV1::Value(
            ValueTreeError::Memory(_)
        ))
    ));
    drop(frame.ordinary_hold.take());
    let view = readmit_cognition_history_view_ordinary_v1(
        frame.selected,
        frame.head,
        frame.roots,
        frame.memory,
        || false,
    )
    .unwrap();
    let index = frame.index.map(|source| {
        readmit_cognition_history_index_ordinary_v1(source, &view, frame.memory, || false).unwrap()
    });
    let hold = frame
        .memory
        .reserve(
            frame.ordinary_limit - frame.memory.used(),
            MemoryClass::Ordinary,
        )
        .unwrap();
    assert!(matches!(
        readmit_cognition_history_current_ordinary_v1(
            frame.binding,
            frame.source(),
            &view,
            frame.roots,
            frame.memory,
            || false,
        ),
        Err(ValueTreeError::Memory(_))
    ));
    drop(hold);
    let polls = Cell::new(0);
    let current = readmit_cognition_history_current_ordinary_v1(
        frame.binding,
        frame.source(),
        &view,
        frame.roots,
        frame.memory,
        || {
            polls.set(polls.get() + 1);
            false
        },
    )
    .unwrap();
    let final_poll = polls.get();
    assert!(final_poll > 1);
    polls.set(0);
    let used = frame.memory.used();
    assert!(matches!(
        readmit_cognition_history_current_ordinary_v1(
            frame.binding,
            frame.source(),
            &view,
            frame.roots,
            frame.memory,
            || {
                polls.set(polls.get() + 1);
                polls.get() == final_poll
            },
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(frame.memory.used(), used);
    let mut wrong = (*frame.roots).clone();
    wrong.simulation_tick += 1;
    assert!(matches!(
        readmit_cognition_history_current_ordinary_v1(
            frame.binding,
            frame.source(),
            &view,
            &wrong,
            frame.memory,
            || false,
        ),
        Err(ValueTreeError::Invalid(_))
    ));
    let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    assert!(matches!(
        readmit_cognition_history_current_ordinary_v1(
            frame.binding,
            frame.source(),
            &view,
            frame.roots,
            &foreign,
            || false,
        ),
        Err(ValueTreeError::Invalid(_))
    ));
    assert_eq!(
        before,
        (
            frame.io.physical.reads_remaining,
            frame.io.physical.read_bytes_remaining,
            frame.io.logical.nodes_remaining,
            frame.io.logical.read_bytes_remaining
        )
    );
    assert_eq!(current.binding().roots_id(), frame.roots.digest().unwrap());
    assert_eq!(
        current.binding().head_record_id(),
        view.occurrence().head_record_id()
    );
    // Inspect every actual slot, including present optional groups, without
    // touching a body. Each copied proof remains bound to its exact field/root.
    if let CognitionHistoryCurrentSourceV1::Enabled(proof) = current.current() {
        let mut present = 0;
        for field in oasis7_world_state::incremental_cognition_json::CognitionJsonFieldV1::ALL {
            if let Some(entry) = proof
                .json_entry(field, &frame.roots.digest().unwrap())
                .unwrap()
            {
                present += 1;
                entry.validate_binding(field, entry.body_root()).unwrap();
            }
        }
        if let Some(expected) = frame.expected_json_slots {
            assert_eq!(present, expected);
        }
    }
    drop(unavailable);
    OrdinaryFixtureProofs {
        view,
        current,
        index,
    }
}

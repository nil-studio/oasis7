//! A zero-allocation archive view over the native journal and host-only sidecar.
use super::super::archive::RuntimeWorldEvent;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct WorldEventRef<'a> {
    pub id: WorldEventId,
    pub time: WorldTime,
    pub kind: &'a WorldEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_event: Option<&'a RuntimeWorldEvent>,
}
impl WorldEventRef<'_> {
    pub fn to_owned(self) -> WorldEvent {
        WorldEvent {
            id: self.id,
            time: self.time,
            kind: self.kind.clone(),
            runtime_event: self.runtime_event.cloned(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WorldJournalView<'a> {
    pub(super) events: &'a [oasis7_simulator::KernelEvent],
    pub(super) runtime_events: &'a std::collections::BTreeMap<usize, RuntimeWorldEvent>,
}
impl<'a> WorldJournalView<'a> {
    pub fn len(self) -> usize {
        self.events.len()
    }
    pub fn is_empty(self) -> bool {
        self.events.is_empty()
    }
    pub fn get(self, index: usize) -> Option<WorldEventRef<'a>> {
        self.events.get(index).map(|event| WorldEventRef {
            id: event.id,
            time: event.time,
            kind: &event.kind,
            runtime_event: self.runtime_events.get(&index),
        })
    }
    pub fn last(self) -> Option<WorldEventRef<'a>> {
        self.len().checked_sub(1).and_then(|index| self.get(index))
    }
    pub fn iter(
        self,
    ) -> impl DoubleEndedIterator<Item = WorldEventRef<'a>> + ExactSizeIterator + 'a {
        (0..self.len()).map(move |index| self.get(index).expect("journal index in bounds"))
    }
}
impl PartialEq for WorldJournalView<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}
impl<'a> IntoIterator for WorldJournalView<'a> {
    type Item = WorldEventRef<'a>;
    type IntoIter = WorldJournalIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        WorldJournalIter {
            view: self,
            range: 0..self.len(),
        }
    }
}

pub struct WorldJournalIter<'a> {
    view: WorldJournalView<'a>,
    range: std::ops::Range<usize>,
}
impl<'a> Iterator for WorldJournalIter<'a> {
    type Item = WorldEventRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.range.next().and_then(|i| self.view.get(i))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}
impl DoubleEndedIterator for WorldJournalIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.range.next_back().and_then(|i| self.view.get(i))
    }
}
impl ExactSizeIterator for WorldJournalIter<'_> {}

impl serde::Serialize for WorldJournalView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

/// Move each native event once; allocate sidecar entries only for actual Runtime events.
pub(super) fn split_events(
    events: Vec<WorldEvent>,
) -> (
    Vec<oasis7_simulator::KernelEvent>,
    std::collections::BTreeMap<usize, RuntimeWorldEvent>,
) {
    let mut native = Vec::with_capacity(events.len());
    let mut runtime = std::collections::BTreeMap::new();
    for (index, event) in events.into_iter().enumerate() {
        let (event, runtime_event) = event.into_kernel_parts();
        native.push(event);
        if let Some(event) = runtime_event {
            runtime.insert(index, event);
        }
    }
    (native, runtime)
}

//! Borrowed semantic input for new-format genesis preparation. This visitor is
//! never the ordinary commit path: mutation entrypoints must emit their actual
//! read/write sets. No World/Snapshot clone or history hydration occurs here.
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldOwner {
    WorldState,
    Runtime,
    Cognition,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldDestination {
    State(StateNamespaceV1),
    History,
    ProofLedger,
    RetentionLedger,
    Process,
    /// A container delegated to its independently exhaustive field visitor.
    Children,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldShape {
    Value,
    Map,
    NestedMap,
    Set,
    MapSet,
    Ordered,
    ByteMap,
    History,
    Process,
    Children,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldDescriptor {
    pub owner: FieldOwner,
    pub name: &'static str,
    pub destination: FieldDestination,
    pub shape: FieldShape,
}

/// A fallible sink owns allocation/IO admission and canonical encoding. Keys
/// are borrowed semantic keys, not a frozen wire layout. Ordered indices are
/// enumeration positions for genesis, never a license to renumber live queues.
/// Side records remain separate from RootSet to avoid current-proof cycles.
pub trait GenesisFieldVisitor {
    type Error;
    fn begin(&mut self, field: FieldDescriptor, entries: Option<usize>) -> Result<(), Self::Error>;
    fn entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error>;
    fn bytes<K: Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        value: &[u8],
    ) -> Result<(), Self::Error>;
    /// Receiving a borrow does not traverse/serialize the historical payload.
    fn side<T: Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        value: &T,
        entries: Option<u64>,
    ) -> Result<(), Self::Error>;
    /// Process credentials/caches are not passed to Serialize or to the sink.
    fn process(&mut self, field: FieldDescriptor) -> Result<(), Self::Error>;
    fn end(&mut self, _field: FieldDescriptor) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum ProjectionError<E> {
    Sink(E),
    Invalid(&'static str),
}

pub fn scalar<V: Serialize + ?Sized, S: GenesisFieldVisitor>(
    sink: &mut S,
    field: FieldDescriptor,
    value: &V,
) -> Result<(), ProjectionError<S::Error>> {
    sink.begin(field, Some(1)).map_err(ProjectionError::Sink)?;
    sink.entry(field, &(), value)
        .map_err(ProjectionError::Sink)?;
    sink.end(field).map_err(ProjectionError::Sink)
}

// Exported only for the Runtime crate's identical borrowed traversal. This
// macro does not define a codec or grant a successful genesis activation.
#[macro_export]
macro_rules! visit_iws_field {
    ($sink:expr, $field:expr, $value:expr, agents) => {{
        let _ = $field;
        $crate::incremental_agent_fields::visit_agents_projection($value, $sink)
    }};
    ($sink:expr, $field:expr, $value:expr, value) => {
        $crate::incremental_projection::scalar($sink, $field, $value)
    };
    ($sink:expr, $field:expr, $value:expr, map) => {{
        use $crate::incremental_projection::ProjectionError;
        let value = $value;
        $sink
            .begin($field, Some(value.len()))
            .map_err(ProjectionError::Sink)?;
        for (key, value) in value.iter() {
            $sink
                .entry($field, key, value)
                .map_err(ProjectionError::Sink)?;
        }
        $sink.end($field).map_err(ProjectionError::Sink)
    }};
    ($sink:expr, $field:expr, $value:expr, nested) => {{
        use $crate::incremental_projection::ProjectionError;
        $sink.begin($field, None).map_err(ProjectionError::Sink)?;
        for (outer, values) in $value.iter() {
            for (key, value) in values {
                $sink
                    .entry($field, &(outer, key), value)
                    .map_err(ProjectionError::Sink)?;
            }
        }
        $sink.end($field).map_err(ProjectionError::Sink)
    }};
    ($sink:expr, $field:expr, $value:expr, mapset) => {{
        use $crate::incremental_projection::ProjectionError;
        $sink.begin($field, None).map_err(ProjectionError::Sink)?;
        for (outer, values) in $value.iter() {
            for key in values {
                $sink
                    .entry($field, &(outer, key), &())
                    .map_err(ProjectionError::Sink)?;
            }
        }
        $sink.end($field).map_err(ProjectionError::Sink)
    }};
    ($sink:expr, $field:expr, $value:expr, set) => {{
        use $crate::incremental_projection::ProjectionError;
        $sink
            .begin($field, Some($value.len()))
            .map_err(ProjectionError::Sink)?;
        for key in $value.iter() {
            $sink
                .entry($field, key, &())
                .map_err(ProjectionError::Sink)?;
        }
        $sink.end($field).map_err(ProjectionError::Sink)
    }};
    ($sink:expr, $field:expr, $value:expr, ordered) => {{
        use $crate::incremental_projection::ProjectionError;
        $sink
            .begin($field, Some($value.len()))
            .map_err(ProjectionError::Sink)?;
        for (index, value) in $value.iter().enumerate() {
            $sink
                .entry($field, &(index as u64), value)
                .map_err(ProjectionError::Sink)?;
        }
        $sink.end($field).map_err(ProjectionError::Sink)
    }};
    ($sink:expr, $field:expr, $value:expr, bytes) => {{
        use $crate::incremental_projection::ProjectionError;
        $sink
            .begin($field, Some($value.len()))
            .map_err(ProjectionError::Sink)?;
        for (key, value) in $value.iter() {
            $sink
                .bytes($field, key, value.as_ref())
                .map_err(ProjectionError::Sink)?;
        }
        $sink.end($field).map_err(ProjectionError::Sink)
    }};
    ($sink:expr, $field:expr, $value:expr, history) => {
        $sink
            .side($field, $value, Some($value.len() as u64))
            .map_err($crate::incremental_projection::ProjectionError::Sink)
    };
    ($sink:expr, $field:expr, $value:expr, side) => {
        $sink
            .side($field, $value, None)
            .map_err($crate::incremental_projection::ProjectionError::Sink)
    };
    ($sink:expr, $field:expr, $value:expr, process) => {
        $sink
            .process($field)
            .map_err($crate::incremental_projection::ProjectionError::Sink)
    };
}

#[path = "incremental_state_projection.rs"]
mod state;
pub use state::WORLD_STATE_FIELDS;
#[path = "incremental_snapshot_fields.rs"]
mod snapshot;
pub use snapshot::{SNAPSHOT_FIELDS, SnapshotFieldLink, SnapshotFieldLinkKind};

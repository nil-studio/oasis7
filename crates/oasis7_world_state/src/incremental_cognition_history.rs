//! Borrowed original Cognition History records. This is not an acceptance
//! authority or a replacement for the formal HistoryPrefix/catalog.
use crate::incremental_cognition_fields::CognitionEnvelopeRefV1;
use oasis7_formal_protocol::incremental_execution::ExecutionFactKindV1;
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::{Map, Value};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionHistoryFieldV1 {
    JournalEvents,
    Responses,
    EconomyJournal,
    EconomyProvisionJournal,
}
impl CognitionHistoryFieldV1 {
    pub const ALL: [Self; 4] = [
        Self::JournalEvents,
        Self::Responses,
        Self::EconomyJournal,
        Self::EconomyProvisionJournal,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::JournalEvents => "journal.events",
            Self::Responses => "responses",
            Self::EconomyJournal => "economy.journal",
            Self::EconomyProvisionJournal => "economy.provision_journal",
        }
    }
    pub fn fact_kind(self) -> ExecutionFactKindV1 {
        match self {
            Self::Responses => ExecutionFactKindV1::Result,
            _ => ExecutionFactKindV1::Event,
        }
    }
    pub(crate) fn domain(self) -> &'static str {
        match self {
            Self::JournalEvents => "oasis7.cognition.event.v1",
            Self::Responses => "oasis7.cognition.response.v1",
            Self::EconomyJournal => "oasis7.cognition.economy.event.v1",
            Self::EconomyProvisionJournal => "oasis7.cognition.economy.provisioning-event.v1",
        }
    }
}
/// The original pre-cognition World may have no projection or JSON null.
/// This borrowed state is explicit; it does not fabricate a default history.
#[derive(Clone, Copy)]
pub enum CognitionHistorySourceRefV1<'a> {
    Disabled,
    Enabled(CognitionEnvelopeRefV1<'a>),
}
impl<'a> CognitionHistorySourceRefV1<'a> {
    pub fn from_original(value: Option<&'a Value>) -> Result<Self, &'static str> {
        match value {
            None | Some(Value::Null) => Ok(Self::Disabled),
            Some(value) => CognitionEnvelopeRefV1::new(value).map(Self::Enabled),
        }
    }
    pub fn enabled(self) -> bool {
        matches!(self, Self::Enabled(_))
    }
}
impl<'a> From<CognitionEnvelopeRefV1<'a>> for CognitionHistorySourceRefV1<'a> {
    fn from(value: CognitionEnvelopeRefV1<'a>) -> Self {
        Self::Enabled(value)
    }
}
#[derive(Clone, Copy)]
pub struct CognitionHistoryCollectionRefV1<'a> {
    field: CognitionHistoryFieldV1,
    records: Option<&'a [Value]>,
}
impl<'a> CognitionHistoryCollectionRefV1<'a> {
    pub fn new(
        field: CognitionHistoryFieldV1,
        source: impl Into<CognitionHistorySourceRefV1<'a>>,
    ) -> Result<Self, &'static str> {
        let CognitionHistorySourceRefV1::Enabled(source) = source.into() else {
            return Ok(Self {
                field,
                records: None,
            });
        };
        let value = match field {
            CognitionHistoryFieldV1::JournalEvents => source.journal_field("events"),
            CognitionHistoryFieldV1::Responses => source.field("responses"),
            CognitionHistoryFieldV1::EconomyJournal => source
                .field("cognition_economy")
                .and_then(|v| v.get("journal")),
            CognitionHistoryFieldV1::EconomyProvisionJournal => source
                .field("cognition_economy")
                .and_then(|v| v.get("provision_journal")),
        };
        let records = value
            .map(|v| {
                v.as_array()
                    .map(Vec::as_slice)
                    .ok_or("Cognition History array")
            })
            .transpose()?;
        Ok(Self { field, records })
    }
    pub fn present(self) -> bool {
        self.records.is_some()
    }
    pub fn len(self) -> usize {
        self.records.map_or(0, <[Value]>::len)
    }
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
    pub fn record(
        self,
        ordinal: usize,
    ) -> Result<Option<CognitionHistoryRecordRefV1<'a>>, &'static str> {
        let Some(value) = self.records.and_then(|records| records.get(ordinal)) else {
            return Ok(None);
        };
        if !value.is_object() {
            return Err("Cognition History record object");
        }
        Ok(Some(CognitionHistoryRecordRefV1 {
            field: self.field,
            ordinal: u64::try_from(ordinal).map_err(|_| "Cognition History ordinal")?,
            value,
        }))
    }
}
#[derive(Clone, Copy)]
pub struct CognitionHistoryRecordRefV1<'a> {
    field: CognitionHistoryFieldV1,
    ordinal: u64,
    value: &'a Value,
}
impl<'a> CognitionHistoryRecordRefV1<'a> {
    /// Borrow one original view row. This validates only its container shape;
    /// its ordinal is view-local and grants no accepted History authority.
    pub fn from_original(
        field: CognitionHistoryFieldV1,
        ordinal: u64,
        value: &'a Value,
    ) -> Result<Self, &'static str> {
        if !value.is_object() {
            return Err("Cognition History record object");
        }
        Ok(Self {
            field,
            ordinal,
            value,
        })
    }
    pub fn field(self) -> CognitionHistoryFieldV1 {
        self.field
    }
    pub fn ordinal(self) -> u64 {
        self.ordinal
    }
    pub fn raw(self) -> &'a Value {
        self.value
    }
    pub fn declared_identity(self) -> Result<Option<&'a str>, &'static str> {
        let name = if self.field == CognitionHistoryFieldV1::Responses {
            "response_digest"
        } else {
            "event_digest"
        };
        match self.value.get(name) {
            None => Ok(None),
            Some(Value::String(value)) => Ok(Some(value)),
            _ => Err("Cognition original identity string"),
        }
    }
    pub(super) fn preimage(self) -> Option<DigestPreimage<'a>> {
        match self.field {
            CognitionHistoryFieldV1::Responses => self
                .value
                .get("response_artifact")
                .filter(|v| !v.is_null())
                .map(|value| DigestPreimage::Artifact(self.field.domain(), value)),
            _ => Some(DigestPreimage::Event(
                self.field.domain(),
                FilteredObject {
                    object: self.value.as_object().expect("borrowed record object"),
                    omit: "event_digest",
                },
            )),
        }
    }
}
/// The original digest projection excludes exactly the existing self digest.
/// Borrow the original Value so absent/null/unknown fields remain unchanged.
pub(super) struct FilteredObject<'a> {
    object: &'a Map<String, Value>,
    omit: &'static str,
}
impl Serialize for FilteredObject<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(
            self.object.len() - usize::from(self.object.contains_key(self.omit)),
        ))?;
        for (key, value) in self.object {
            if key != self.omit {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
}
pub(super) enum DigestPreimage<'a> {
    Event(&'static str, FilteredObject<'a>),
    Artifact(&'static str, &'a Value),
}
impl Serialize for DigestPreimage<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Event(domain, value) => (domain, value).serialize(serializer),
            Self::Artifact(domain, value) => (domain, value).serialize(serializer),
        }
    }
}
#[path = "incremental_cognition_history_original.rs"]
mod original;
pub use original::*;
#[path = "incremental_cognition_history_manifest.rs"]
mod manifest;
pub use manifest::*;
#[path = "incremental_cognition_history_selection.rs"]
mod selection;
pub use selection::*;
#[path = "incremental_cognition_history_current.rs"]
mod current;
pub use current::*;
#[path = "incremental_cognition_history_complete_v3.rs"]
mod complete_v3;
pub use complete_v3::*;
#[path = "incremental_cognition_history_response_append.rs"]
mod response_append;
pub use response_append::*;
#[path = "incremental_cognition_history_index.rs"]
mod index;
pub use index::*;
#[path = "incremental_cognition_history_indexed_original.rs"]
mod indexed_original;
pub use indexed_original::*;

#[cfg(test)]
#[path = "incremental_cognition_history_original_tests.rs"]
mod tests;

#[path = "incremental_cognition_history_response_successor.rs"]
mod response_successor;
pub use response_successor::*;

#[path = "incremental_cognition_history_head_cache.rs"]
mod head_cache;
pub use head_cache::*;

#[path = "incremental_cognition_history_manifest_v2.rs"]
mod manifest_v2;
pub use manifest_v2::*;

#[path = "incremental_cognition_history_head_cache_store.rs"]
mod head_cache_store;
pub use head_cache_store::*;

#[path = "incremental_cognition_history_components.rs"]
mod components;
pub use components::*;

#[path = "incremental_cognition_history_source_stream.rs"]
mod source_stream;
pub use source_stream::*;

#[path = "incremental_cognition_history_source_summary_v3.rs"]
mod source_summary_v3;
pub use source_summary_v3::*;
#[path = "incremental_cognition_history_manifest_v3.rs"]
mod manifest_v3;
pub use manifest_v3::*;
#[path = "incremental_cognition_history_candidate_original.rs"]
mod candidate_original;
pub use candidate_original::*;

#[path = "incremental_cognition_history_attested_index_v3.rs"]
mod attested_index_v3;
pub use attested_index_v3::*;
#[path = "incremental_cognition_history_disabled_v3.rs"]
mod disabled_v3;
pub use disabled_v3::*;
#[path = "incremental_cognition_history_dependencies_v3.rs"]
mod dependencies_v3;
pub use dependencies_v3::*;

#[path = "incremental_cognition_history_candidate_complete_reader_v3.rs"]
mod candidate_complete_reader_v3;
pub use candidate_complete_reader_v3::*;

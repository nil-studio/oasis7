//! Closed ownership of the original Cognition JSON envelope. This registry is
//! structural routing, not a cognition/receipt/history acceptance capability.
use crate::incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1 as Ns;
use serde_json::{Map, Value};

pub const REQUIRED_COGNITION_FIELDS: [&str; 12] = [
    "schema_version",
    "cognition_journal",
    "responses",
    "commit_records",
    "receipt_registry",
    "receipt_lineage_registry",
    "feedback_outbox",
    "idempotency_index",
    "staged_actions",
    "scheduler_state",
    "continuations",
    "recovery",
];
pub const OPTIONAL_COGNITION_FIELDS: [&str; 8] = [
    "runtime_binding",
    "retention_state",
    "continuation_contexts",
    "cognition_economy",
    "agent_delegation",
    "agent_delegation_active_decision",
    "continuation_evaluations",
    "reorg_invalidation_epoch",
];

pub const COGNITION_ECONOMY_FIELDS: [&str; 16] = [
    "schema_version",
    "balances",
    "provisioned_balances",
    "lease_binding_keys",
    "provisions",
    "provision_receipts",
    "provision_journal",
    "provision_head_seq",
    "provision_head_digest",
    "leases",
    "receipts",
    "idempotency",
    "operations",
    "journal",
    "head_seq",
    "head_digest",
];
pub const COGNITION_DELEGATION_FIELDS: [&str; 5] =
    ["grants", "spent", "decisions", "receipts", "controls"];

/// Borrow the original envelope without deserializing, cloning, normalizing IDs
/// or reinterpreting side-owner records as current state.
#[derive(Clone, Copy)]
pub struct CognitionEnvelopeRefV1<'a> {
    object: &'a Map<String, Value>,
    journal: &'a Map<String, Value>,
}
impl<'a> CognitionEnvelopeRefV1<'a> {
    pub fn new(value: &'a Value) -> Result<Self, &'static str> {
        let object = value.as_object().ok_or("cognition object")?;
        if object.keys().any(|k| {
            !REQUIRED_COGNITION_FIELDS.contains(&k.as_str())
                && !OPTIONAL_COGNITION_FIELDS.contains(&k.as_str())
        }) || REQUIRED_COGNITION_FIELDS
            .iter()
            .any(|k| !object.contains_key(*k))
        {
            return Err("unclassified or missing cognition field");
        }
        if object.contains_key("agent_delegation_active_decision") {
            return Err("active delegation decision is transaction input, not genesis state");
        }
        if object["schema_version"].as_str() != Some("cognition-persistence.v1") {
            return Err("cognition schema");
        }
        let journal = object["cognition_journal"]
            .as_object()
            .ok_or("cognition journal object")?;
        let keys = ["schema_version", "head_seq", "head_digest", "events"];
        if journal.len() != keys.len()
            || keys.iter().any(|k| !journal.contains_key(*k))
            || journal["schema_version"].as_str() != Some("cognition-journal.v1")
            || journal["head_seq"].as_u64().is_none()
            || journal["head_digest"].as_str().is_none()
            || !journal["events"].is_array()
        {
            return Err("unclassified or invalid cognition journal field");
        }
        for key in [
            "responses",
            "commit_records",
            "receipt_registry",
            "receipt_lineage_registry",
            "continuations",
        ] {
            if !object[key].is_array() {
                return Err("cognition array shape");
            }
        }
        for key in ["feedback_outbox", "idempotency_index", "staged_actions"] {
            if !object[key].is_object() {
                return Err("cognition state map shape");
            }
        }
        for key in ["continuation_contexts", "continuation_evaluations"] {
            if object.get(key).is_some_and(|v| !v.is_object()) {
                return Err("cognition optional map shape");
            }
        }
        if object
            .get("reorg_invalidation_epoch")
            .is_some_and(|v| v.as_u64().is_none())
        {
            return Err("reorg invalidation epoch shape");
        }
        validate_optional_owners(object)?;
        Ok(Self { object, journal })
    }
    pub fn field(self, name: &str) -> Option<&'a Value> {
        self.object.get(name)
    }
    pub fn journal_field(self, name: &str) -> Option<&'a Value> {
        self.journal.get(name)
    }
}

fn validate_optional_owners(object: &Map<String, Value>) -> Result<(), &'static str> {
    if let Some(value) = object.get("cognition_economy") {
        let economy = value.as_object().ok_or("economy object")?;
        if economy.len() != COGNITION_ECONOMY_FIELDS.len()
            || COGNITION_ECONOMY_FIELDS
                .iter()
                .any(|key| !economy.contains_key(*key))
        {
            return Err("unclassified or missing economy field");
        }
        if economy["schema_version"].as_str() != Some("cognition-economy.v1") {
            return Err("economy schema");
        }
        for key in [
            "balances",
            "provisioned_balances",
            "lease_binding_keys",
            "provisions",
            "provision_receipts",
            "leases",
            "receipts",
            "idempotency",
            "operations",
        ] {
            if !economy[key].is_object() {
                return Err("economy map shape");
            }
        }
        for key in ["journal", "provision_journal"] {
            if !economy[key].is_array() {
                return Err("economy history shape");
            }
        }
        for key in ["head_seq", "provision_head_seq"] {
            if economy[key].as_u64().is_none() {
                return Err("economy cursor shape");
            }
        }
        for key in ["head_digest", "provision_head_digest"] {
            if !economy[key].is_string() {
                return Err("economy digest shape");
            }
        }
    }
    if let Some(value) = object.get("agent_delegation") {
        let delegation = value.as_object().ok_or("delegation object")?;
        if delegation.len() != COGNITION_DELEGATION_FIELDS.len()
            || COGNITION_DELEGATION_FIELDS
                .iter()
                .any(|key| !delegation.contains_key(*key))
        {
            return Err("unclassified or missing delegation field");
        }
        for key in COGNITION_DELEGATION_FIELDS {
            if !delegation[key].is_object() {
                return Err("delegation map shape");
            }
        }
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CognitionControlFieldV1 {
    Schema,
    JournalSchema,
    JournalHeadSeq,
    JournalHeadDigest,
    ReorgInvalidationEpoch,
    EconomySchema,
    EconomyProvisionHeadSeq,
    EconomyProvisionHeadDigest,
    EconomyHeadSeq,
    EconomyHeadDigest,
}
impl CognitionControlFieldV1 {
    pub const REQUIRED: [Self; 4] = [
        Self::Schema,
        Self::JournalSchema,
        Self::JournalHeadSeq,
        Self::JournalHeadDigest,
    ];
    pub const ALL: [Self; 10] = [
        Self::Schema,
        Self::JournalSchema,
        Self::JournalHeadSeq,
        Self::JournalHeadDigest,
        Self::ReorgInvalidationEpoch,
        Self::EconomySchema,
        Self::EconomyProvisionHeadSeq,
        Self::EconomyProvisionHeadDigest,
        Self::EconomyHeadSeq,
        Self::EconomyHeadDigest,
    ];
    pub fn descriptor(self) -> FieldDescriptor {
        let (name, namespace) = match self {
            Self::Schema => ("schema_version", Ns::Artifacts),
            Self::JournalSchema => ("journal.schema_version", Ns::Artifacts),
            Self::JournalHeadSeq => ("journal.head_seq", Ns::Allocator),
            Self::JournalHeadDigest => ("journal.head_digest", Ns::Allocator),
            Self::ReorgInvalidationEpoch => ("reorg_invalidation_epoch", Ns::Authorization),
            Self::EconomySchema => ("economy.schema_version", Ns::Artifacts),
            Self::EconomyProvisionHeadSeq => ("economy.provision_head_seq", Ns::Allocator),
            Self::EconomyProvisionHeadDigest => ("economy.provision_head_digest", Ns::Allocator),
            Self::EconomyHeadSeq => ("economy.head_seq", Ns::Allocator),
            Self::EconomyHeadDigest => ("economy.head_digest", Ns::Allocator),
        };
        FieldDescriptor {
            owner: FieldOwner::Cognition,
            name,
            destination: FieldDestination::State(namespace),
            shape: FieldShape::Value,
        }
    }
    pub fn optional(self) -> bool {
        matches!(
            self,
            Self::ReorgInvalidationEpoch
                | Self::EconomySchema
                | Self::EconomyProvisionHeadSeq
                | Self::EconomyProvisionHeadDigest
                | Self::EconomyHeadSeq
                | Self::EconomyHeadDigest
        )
    }
    pub fn unsigned(self) -> bool {
        matches!(
            self,
            Self::JournalHeadSeq
                | Self::ReorgInvalidationEpoch
                | Self::EconomyProvisionHeadSeq
                | Self::EconomyHeadSeq
        )
    }
    pub fn value(self, source: CognitionEnvelopeRefV1<'_>) -> Option<&Value> {
        match self {
            Self::Schema => source.field("schema_version"),
            Self::JournalSchema => source.field("cognition_journal")?.get("schema_version"),
            Self::JournalHeadSeq => source.field("cognition_journal")?.get("head_seq"),
            Self::JournalHeadDigest => source.field("cognition_journal")?.get("head_digest"),
            Self::ReorgInvalidationEpoch => source.field("reorg_invalidation_epoch"),
            Self::EconomySchema => source.field("cognition_economy")?.get("schema_version"),
            Self::EconomyProvisionHeadSeq => {
                source.field("cognition_economy")?.get("provision_head_seq")
            }
            Self::EconomyProvisionHeadDigest => source
                .field("cognition_economy")?
                .get("provision_head_digest"),
            Self::EconomyHeadSeq => source.field("cognition_economy")?.get("head_seq"),
            Self::EconomyHeadDigest => source.field("cognition_economy")?.get("head_digest"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Value {
        serde_json::json!({
            "schema_version":"cognition-persistence.v1",
            "cognition_journal":{"schema_version":"cognition-journal.v1","head_seq":u64::MAX,"head_digest":"原ID","events":[]},
            "responses":[], "commit_records":[], "receipt_registry":[], "receipt_lineage_registry":[],
            "feedback_outbox":{}, "idempotency_index":{}, "staged_actions":{}, "scheduler_state":{}, "continuations":[], "recovery":null
        })
    }
    #[test]
    fn borrowed_registry_preserves_extremes_and_side_owner_values() {
        let mut value = source();
        value["reorg_invalidation_epoch"] = Value::from(u64::MAX);
        let view = CognitionEnvelopeRefV1::new(&value).unwrap();
        assert!(std::ptr::eq(
            view.field("responses").unwrap(),
            &value["responses"]
        ));
        assert_eq!(
            CognitionControlFieldV1::JournalHeadSeq
                .value(view)
                .unwrap()
                .as_u64(),
            Some(u64::MAX)
        );
        assert_eq!(
            CognitionControlFieldV1::JournalHeadDigest
                .value(view)
                .unwrap()
                .as_str(),
            Some("原ID")
        );
        assert_eq!(
            CognitionControlFieldV1::ReorgInvalidationEpoch
                .descriptor()
                .destination,
            FieldDestination::State(Ns::Authorization)
        );
    }
    #[test]
    fn registry_rejects_unknown_missing_wrong_shapes_and_ephemeral_authority() {
        for key in REQUIRED_COGNITION_FIELDS {
            let mut value = source();
            value.as_object_mut().unwrap().remove(key);
            assert!(CognitionEnvelopeRefV1::new(&value).is_err(), "{key}");
        }
        for (key, wrong) in [
            ("unknown", Value::Null),
            ("agent_delegation_active_decision", Value::Null),
            ("continuation_contexts", Value::Array(vec![])),
            ("reorg_invalidation_epoch", Value::from(-1)),
        ] {
            let mut value = source();
            value[key] = wrong;
            assert!(CognitionEnvelopeRefV1::new(&value).is_err(), "{key}");
        }
        let mut value = source();
        value["cognition_journal"]["extra"] = Value::Null;
        assert!(CognitionEnvelopeRefV1::new(&value).is_err());
    }
}

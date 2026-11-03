//! Exact typed original private Data codecs; storage and authorization remain separate.
use super::*;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Data {
    pub(super) schema_version: u32,
    pub(super) domain: String,
    pub(super) origin: bool,
    pub(super) ledger_origin_digest: String,
    pub(super) material_identity: String,
    pub(super) instance_id: String,
    pub(super) height: u64,
    pub(super) value: Option<BftHeaderValueV3>,
    #[serde(deserialize_with = "super::super::super::super::private::deserialize_actions")]
    pub(super) actions_cbor: Vec<u8>,
    #[serde(deserialize_with = "bounded_references")]
    pub(super) references: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) publication_reservation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) publication_original_data_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) source_activation: Option<SourceActivationBindingV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) protected_source_origin_ref: Option<String>,
}
impl Data {
    pub(super) fn validate(&self) -> Result<(), String> {
        if !matches!(
            (self.schema_version, self.domain.as_str()),
            (3, DOMAIN)
                | (4, MULTI_DOMAIN)
                | (5, SELECTION_DOMAIN)
                | (6 | 7, MIGRATED_DOMAIN)
                | (8, COVERED_SOURCE_ORIGIN_DOMAIN)
                | (9, PROTECTED_INPUT_DOMAIN)
                | (10, PROTECTED_RECEIVED_INPUT_DOMAIN)
                | (11, ACTIVE_LOCAL_INPUT_DOMAIN)
                | (12, ACTIVE_RECEIVED_INPUT_DOMAIN)
                | (13, ACTIVE_SELECTION_DOMAIN)
        ) || self.height == 0
            || !canonical_hash(&self.ledger_origin_digest)
            || !canonical_hash(&self.material_identity)
            || !canonical_hash(&self.instance_id)
            || self
                .publication_reservation_ref
                .as_ref()
                .is_some_and(|id| !canonical_hash(id))
            || (matches!(self.schema_version, 6 | 7)
                != self.publication_original_data_ref.is_some())
            || self
                .publication_original_data_ref
                .as_ref()
                .is_some_and(|id| !canonical_hash(id))
            || (self.publication_original_data_ref.is_some()
                && self.publication_reservation_ref.is_none())
            || ((self.origin || matches!(self.schema_version, 5 | 13))
                && self.publication_reservation_ref.is_some())
            || self.references.len()
                > super::super::super::super::super::formal_dependencies::MAX_RETAINED_INDEX_NODES
            || !self.references.windows(2).all(|pair| pair[0] < pair[1])
            || self
                .references
                .iter()
                .any(|reference| !canonical_hash(reference))
        {
            return Err("invalid original Node private record identity or capacity".into());
        }
        if matches!(self.schema_version, 9 | 10 | 11 | 12)
            && self.publication_reservation_ref.is_none()
        {
            return Err("protected source input requires original publication reservation".into());
        }
        if (matches!(self.schema_version, 9 | 10 | 11 | 12))
            != self.protected_source_origin_ref.is_some()
            || self
                .protected_source_origin_ref
                .as_ref()
                .is_some_and(|reference| !canonical_hash(reference))
        {
            return Err("protected input private origin binding differs".into());
        }
        if (self.schema_version == 8) != self.source_activation.is_some() {
            return Err("covered private origin activation schema differs".into());
        }
        if let Some(activation) = &self.source_activation {
            activation.validate()?;
        }
        if matches!(self.schema_version, 5 | 13) {
            if self.origin || !self.actions_cbor.is_empty() || self.references.len() != 1 {
                return Err("invalid original local selection".into());
            }
            let value = self
                .value
                .as_ref()
                .ok_or("local selection has no public value")?;
            value.validate_binding()?;
            if value.execution_commitment.height != self.height {
                return Err("local selection height differs".into());
            }
        } else if self.origin {
            if !matches!(self.schema_version, 3 | 8) {
                return Err("foreign Node origin schema".into());
            }
            if self.value.is_some() || !self.actions_cbor.is_empty() {
                return Err("Node H retention record contains candidate data".into());
            }
        } else {
            if self.source_activation.is_some() {
                return Err("activation on non-origin private record".into());
            }
            let value = self
                .value
                .as_ref()
                .ok_or("Node candidate has no public value")?;
            if value.execution_commitment.height != self.height {
                return Err("Node private candidate height differs from commitment".into());
            }
            ValidatedBftPrivatePayloadV3::decode(value.validate_binding()?, &self.actions_cbor)
                .map_err(|_| "original Node private candidate binding rejected")?;
        }
        Ok(())
    }
    pub(super) fn bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_cbor::to_vec(self).map_err(|_| "encode original private Node record")?;
        if bytes.len() > MAX_RECORD {
            return Err("private Node record byte capacity".into());
        }
        Ok(bytes)
    }
    pub(super) fn digest(&self) -> Result<String, String> {
        Ok(blake3_hex(&self.bytes()?))
    }
    pub(super) fn retained(&self) -> Result<BTreeSet<String>, String> {
        Ok(self
            .references
            .iter()
            .cloned()
            .chain(self.publication_reservation_ref.iter().cloned())
            .chain(self.publication_original_data_ref.iter().cloned())
            .chain(self.source_activation.iter().map(|a| a.reference.clone()))
            .chain(self.protected_source_origin_ref.iter().cloned())
            .chain([self.digest()?])
            .collect())
    }
    pub(super) fn shard(&self) -> String {
        if matches!(self.schema_version, 5 | 13) {
            format!(
                "ledger-{}-selection-{:020}",
                self.ledger_origin_digest, self.height
            )
        } else if matches!(self.schema_version, 4 | 7 | 10 | 12) {
            format!(
                "ledger-{}-height-{:020}-header-{}",
                self.ledger_origin_digest,
                self.height,
                self.value
                    .as_ref()
                    .expect("validated candidate")
                    .header
                    .block_id()
                    .expect("validated header")
            )
        } else if self.origin {
            format!("ledger-{}-origin", self.ledger_origin_digest)
        } else {
            format!(
                "ledger-{}-height-{:020}",
                self.ledger_origin_digest, self.height
            )
        }
    }
}

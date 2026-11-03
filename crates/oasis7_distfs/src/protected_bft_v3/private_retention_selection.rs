//! Protected current-choice identities. Canonical bytes alone never grant a choice.
use super::binding::{BftSignerErrorV3, invalid};
use super::crypto::digest_hex;
use super::private_retention::{BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1};
use super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSelectRequestV1 {
    pub operation_id: String,
    pub key: BftPrivateRetentionKeyV1,
    pub reference: BftPrivateRetentionAdmissionInputRefV1,
    pub expected_revision: u64,
    pub expected_state_digest: String,
    pub expected_head_digest: String,
    pub expected_choice: Option<String>,
}
impl BftPrivateRetentionSelectRequestV1 {
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.reference.validate()?;
        for value in [
            &self.operation_id,
            &self.expected_state_digest,
            &self.expected_head_digest,
        ] {
            super::private_retention::hash_value(value)?;
        }
        if let Some(choice) = &self.expected_choice {
            super::private_retention::hash_value(choice)?;
        }
        if self.key != self.reference.key {
            return Err(invalid("selection request key/reference differs"));
        }
        digest_hex("oasis7.bft.private-retention.select-operation.v1", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum BftPrivateRetentionSelectionOperationV1 {
    ReadyOriginalLocal {
        reference: BftPrivateRetentionAdmissionInputRefV1,
        expected_revision: u64,
        expected_state_digest: String,
        expected_head_digest: String,
    },
    Select {
        request: BftPrivateRetentionSelectRequestV1,
    },
}
impl BftPrivateRetentionSelectionOperationV1 {
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        match self {
            Self::ReadyOriginalLocal {
                reference,
                expected_state_digest,
                expected_head_digest,
                ..
            } => {
                reference.validate()?;
                super::private_retention::hash_value(expected_state_digest)?;
                super::private_retention::hash_value(expected_head_digest)?;
                digest_hex("oasis7.bft.private-retention.original-local-ready.v1", self)
            }
            Self::Select { request } => request.digest(),
        }
    }
    pub fn reference(&self) -> &BftPrivateRetentionAdmissionInputRefV1 {
        match self {
            Self::ReadyOriginalLocal { reference, .. } => reference,
            Self::Select { request } => &request.reference,
        }
    }
    fn expected(&self) -> (u64, &str, &str) {
        match self {
            Self::ReadyOriginalLocal {
                expected_revision,
                expected_state_digest,
                expected_head_digest,
                ..
            } => (
                *expected_revision,
                expected_state_digest,
                expected_head_digest,
            ),
            Self::Select { request } => (
                request.expected_revision,
                &request.expected_state_digest,
                &request.expected_head_digest,
            ),
        }
    }
}

pub struct BftPrivateRetentionSelectionResultV1 {
    pub current: super::private_retention_backend::BftPrivateRetentionLedgerV1,
    pub original: super::private_retention_operation_proof::BftPrivateRetentionOperationProofV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSelectionCommitmentV1 {
    pub root: String,
    pub encoded_bytes: u64,
    pub count: u32,
    pub sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BftPrivateRetentionSelectionKindV1 {
    OriginalLocal,
    ReceivedLocal,
}

/// Constructed by the configured independent material verifier. This is only
/// its typed result, never a ReadGrant or a serializable authority token.
pub struct VerifiedBftPrivateRetentionSelectionIdentityV1 {
    kind: BftPrivateRetentionSelectionKindV1,
    digest: String,
    bytes: u64,
}
impl VerifiedBftPrivateRetentionSelectionIdentityV1 {
    pub fn from_independent_verification(
        kind: BftPrivateRetentionSelectionKindV1,
        digest: String,
        bytes: u64,
    ) -> Result<Self, BftSignerErrorV3> {
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || bytes == 0
            || bytes > 4 * 1024 * 1024
        {
            return Err(invalid("verified selection identity shape differs"));
        }
        Ok(Self {
            kind,
            digest,
            bytes,
        })
    }
    pub fn kind(&self) -> BftPrivateRetentionSelectionKindV1 {
        self.kind
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSelectionV1 {
    pub source: BftPrivateRetentionSourceV1,
    pub key: BftPrivateRetentionKeyV1,
    pub reference: BftPrivateRetentionAdmissionInputRefV1,
    pub kind: BftPrivateRetentionSelectionKindV1,
    pub local_identity_digest: String,
    pub local_identity_bytes: u64,
    pub sequence: u64,
    pub operation_digest: String,
    pub operation: BftPrivateRetentionSelectionOperationV1,
}
impl BftPrivateRetentionSelectionV1 {
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        digest_hex("oasis7.bft.private-retention.selection.v1", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSelectionIndexV1 {
    pub source: BftPrivateRetentionSourceV1,
    pub last_sequence: u64,
    #[serde(deserialize_with = "deserialize_selections")]
    pub selections: Vec<BftPrivateRetentionSelectionV1>,
}

fn deserialize_selections<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<BftPrivateRetentionSelectionV1>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<BftPrivateRetentionSelectionV1>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 256 original protected choices")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > 256) {
                return Err(serde::de::Error::custom("selection index count capacity"));
            }
            let mut result = Vec::new();
            while let Some(item) = seq.next_element()? {
                if result.len() == 256 {
                    return Err(serde::de::Error::custom("selection index count capacity"));
                }
                result.push(item);
            }
            Ok(result)
        }
    }
    d.deserialize_seq(Visitor)
}
impl BftPrivateRetentionSelectionIndexV1 {
    pub fn commitment(&self) -> Result<BftPrivateRetentionSelectionCommitmentV1, BftSignerErrorV3> {
        Ok(BftPrivateRetentionSelectionCommitmentV1 {
            root: self.digest()?,
            encoded_bytes: super::crypto::encode_bounded(
                self,
                super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
            )?
            .len() as u64,
            count: self.selections.len() as u32,
            sequence: self.last_sequence,
        })
    }
    pub(crate) fn validate(
        &self,
        map: &super::private_retention::BftPrivateRetentionMapV1,
    ) -> Result<(), BftSignerErrorV3> {
        if self.source != map.source
            || self.last_sequence == 0
            || self.selections.len()
                > super::private_retention::BFT_PRIVATE_RETENTION_MAX_ENTRIES_V1
        {
            return Err(invalid("selection index source/count differs"));
        }
        let mut previous = None;
        let mut sequences = std::collections::BTreeSet::new();
        for selection in &self.selections {
            let identity = (&selection.key.ledger_origin_digest, selection.key.height);
            if previous.is_some_and(|p| p >= identity)
                || selection.source != self.source
                || selection.sequence == 0
                || selection.sequence > self.last_sequence
                || !sequences.insert(selection.sequence)
            {
                return Err(invalid("selection index uniqueness/order differs"));
            }
            previous = Some(identity);
            super::private_retention::hash_value(&selection.local_identity_digest)?;
            super::private_retention::hash_value(&selection.operation_digest)?;
            if selection.local_identity_bytes == 0
                || selection.local_identity_bytes > 4 * 1024 * 1024
            {
                return Err(invalid("selection local material size differs"));
            }
            let entry = map
                .entries
                .iter()
                .find(|e| e.key == selection.key)
                .ok_or_else(|| invalid("selection original absent"))?;
            if entry.lifecycle != super::private_retention::BftPrivateRetentionLifecycleV1::Ready
                || selection.reference
                    != BftPrivateRetentionAdmissionInputRefV1::from_entry(&map.source, entry)?
            {
                return Err(invalid("selection exact Ready reference differs"));
            }
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        digest_hex("oasis7.bft.private-retention.selection-index.v1", self)
    }
}

pub(crate) fn validate_ledger(
    ledger: &super::private_retention_backend::BftPrivateRetentionLedgerV1,
) -> Result<(), BftSignerErrorV3> {
    match (
        &ledger.selection_index,
        &ledger.receipt.body.head.selection_commitment,
    ) {
        (None, None) if ledger.receipt.body.head.schema_version < 3 => Ok(()),
        (Some(index), Some(commitment)) if ledger.receipt.body.head.schema_version == 3 => {
            index.validate(&ledger.map)?;
            if &index.commitment()? != commitment {
                return Err(invalid("selection index commitment differs"));
            }
            Ok(())
        }
        _ => Err(invalid("selection inventory/head version differs")),
    }
}

pub(crate) fn select(
    previous: Option<&BftPrivateRetentionSelectionIndexV1>,
    source: &BftPrivateRetentionSourceV1,
    reference: BftPrivateRetentionAdmissionInputRefV1,
    identity: VerifiedBftPrivateRetentionSelectionIdentityV1,
    operation: BftPrivateRetentionSelectionOperationV1,
) -> Result<(BftPrivateRetentionSelectionIndexV1, bool), BftSignerErrorV3> {
    let mut index = previous
        .cloned()
        .unwrap_or_else(|| BftPrivateRetentionSelectionIndexV1 {
            source: source.clone(),
            last_sequence: 0,
            selections: Vec::new(),
        });
    if &index.source != source || &reference.source != source {
        return Err(invalid("selection original source differs"));
    }
    if let Some(old) = index.selections.iter().find(|s| {
        s.key.ledger_origin_digest == reference.key.ledger_origin_digest
            && s.key.height == reference.key.height
    }) {
        if old.reference != reference
            || old.kind != identity.kind
            || old.local_identity_digest != identity.digest
            || old.local_identity_bytes != identity.bytes
        {
            return Err(invalid("protected first choice already differs"));
        }
        return Ok((index, false));
    }
    let sequence = index
        .last_sequence
        .checked_add(1)
        .ok_or_else(|| invalid("selection sequence overflow"))?;
    index.last_sequence = sequence;
    let operation_digest = operation.digest()?;
    if operation.reference() != &reference {
        return Err(invalid("selection operation reference differs"));
    }
    index.selections.push(BftPrivateRetentionSelectionV1 {
        source: source.clone(),
        key: reference.key.clone(),
        reference,
        kind: identity.kind,
        local_identity_digest: identity.digest,
        local_identity_bytes: identity.bytes,
        sequence,
        operation_digest,
        operation,
    });
    index.selections.sort_by(|a, b| {
        (&a.key.ledger_origin_digest, a.key.height)
            .cmp(&(&b.key.ledger_origin_digest, b.key.height))
    });
    Ok((index, true))
}

/// Authenticate the selected original receipt under a separately checked
/// current owner. This is not a substitute for full archive history replay.
pub(crate) fn validate_original_selection(
    binding: &super::binding::BftSignerBindingV3,
    trusted: &super::private_retention::BftPrivateRetentionAttestorV1,
    current: &super::private_retention_backend::BftPrivateRetentionLedgerV1,
    state: &super::state::BftSignerStateV3,
    original: &super::private_retention_backend::BftPrivateRetentionLedgerV1,
    selected: &BftPrivateRetentionSelectionV1,
) -> Result<(), BftSignerErrorV3> {
    validate_ledger(current)?;
    validate_ledger(original)?;
    super::private_retention_seal::verify_map_at_floor(
        trusted,
        original,
        original.map.coverage_start_height,
    )?;
    state.validate(binding, &trusted.provider_public_key)?;
    let body = &original.receipt.body;
    let (revision, digest, head) = selected.operation.expected();
    super::private_retention::hash_value(head)?;
    if current
        .selection_index
        .as_ref()
        .is_none_or(|i| !i.selections.contains(selected))
        || original.selection_index.as_ref().is_none_or(|i| {
            !i.selections.contains(selected) || i.last_sequence != selected.sequence
        })
        || body.operation_digest != selected.operation_digest
        || selected.operation.digest()? != selected.operation_digest
        || selected.operation.reference() != &selected.reference
        || body.before_revision != revision
        || body.before_state_digest != digest
        || state.revision != body.after_revision
        || state.state_digest != body.after_state_digest
        || state.private_retention.as_ref() != Some(&body.head)
        || selected.source != trusted.source
        || selected.reference.source != trusted.source
    {
        return Err(invalid(
            "original selection receipt/current binding differs",
        ));
    }
    Ok(())
}

pub(crate) fn preflight_original_operation(
    state: &super::state::BftSignerStateV3,
    ledger: &super::private_retention_backend::BftPrivateRetentionLedgerV1,
) -> Result<(), BftSignerErrorV3> {
    #[derive(Serialize)]
    struct Borrow<'a> {
        state: &'a super::state::BftSignerStateV3,
        ledger: &'a super::private_retention_backend::BftPrivateRetentionLedgerV1,
    }
    super::retention_proof_query::canonical_chunk(&Borrow { state, ledger }, 0)?;
    Ok(())
}

pub(crate) fn validate_extension(
    old: Option<&super::private_retention_backend::BftPrivateRetentionLedgerV1>,
    next: &super::private_retention_backend::BftPrivateRetentionLedgerV1,
) -> Result<(), BftSignerErrorV3> {
    validate_ledger(next)?;
    let prior = old.and_then(|l| l.selection_index.as_ref());
    let current = next.selection_index.as_ref();
    if prior == current {
        return Ok(());
    }
    let current =
        current.ok_or_else(|| invalid("ordinary operation removed selection inventory"))?;
    let old_sequence = prior.map_or(0, |i| i.last_sequence);
    if current.last_sequence
        != old_sequence
            .checked_add(1)
            .ok_or_else(|| invalid("selection sequence overflow"))?
        || current.selections.len() != prior.map_or(0, |i| i.selections.len()) + 1
    {
        return Err(invalid("selection history sequence/count differs"));
    }
    if let Some(prior) = prior {
        for item in &prior.selections {
            if !current.selections.contains(item) {
                return Err(invalid("ordinary operation replaced original choice"));
            }
        }
    }
    let added = current
        .selections
        .iter()
        .find(|s| s.sequence == current.last_sequence)
        .ok_or_else(|| invalid("selection original transition absent"))?;
    if added.operation_digest != next.receipt.body.operation_digest {
        return Err(invalid("selection operation/receipt differs"));
    }
    if added.operation.digest()? != added.operation_digest
        || added.operation.reference() != &added.reference
    {
        return Err(invalid("selection typed original operation differs"));
    }
    let old = old.ok_or_else(|| invalid("selection lacks original reservation operation"))?;
    let (revision, digest, head_digest) = added.operation.expected();
    if revision != next.receipt.body.before_revision
        || digest != next.receipt.body.before_state_digest
        || head_digest != old.receipt.body.head.digest()?
    {
        return Err(invalid(
            "selection typed operation before State/head differs",
        ));
    }
    match &added.operation {
        BftPrivateRetentionSelectionOperationV1::ReadyOriginalLocal { .. }
            if added.kind == BftPrivateRetentionSelectionKindV1::OriginalLocal => {}
        BftPrivateRetentionSelectionOperationV1::Select { request }
            if request.key == added.key && request.expected_choice.is_none() =>
        {
            super::private_retention::hash_value(&request.operation_id)?;
        }
        _ => {
            return Err(invalid(
                "selection typed operation kind/first choice differs",
            ));
        }
    }
    {
        if next.map.entries.len() != old.map.entries.len() {
            return Err(invalid("selection operation added admission"));
        }
        for entry in &old.map.entries {
            let new = next
                .map
                .entries
                .iter()
                .find(|e| e.key == entry.key)
                .ok_or_else(|| invalid("selection omitted original"))?;
            if new.fact != entry.fact
                || (new.lifecycle != entry.lifecycle
                    && (entry.key != added.key
                        || new.lifecycle
                            != super::private_retention::BftPrivateRetentionLifecycleV1::Ready))
            {
                return Err(invalid(
                    "selection changed unrelated original fact/lifecycle",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "private_retention_selection_tests.rs"]
mod tests;

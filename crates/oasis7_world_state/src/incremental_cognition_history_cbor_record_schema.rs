//! Per-original fields only. Unknown JSON remains in the original projections;
//! no lifecycle, prefix, accepted occurrence or provider authority is minted.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionHistoryRecordScopeV1 {
    LocalJournalFields,
    LocalEconomyFields,
    LocalProvisionFields,
    /// Artifact JSON is canonical and its original digest is checked, but a
    /// nested RuntimeResponseArtifact business model is NOT verified here.
    ResponseWrapperFields,
}
#[derive(Debug)]
pub struct CognitionHistoryRecordSchemaV1 {
    semantic: CognitionOriginalSemanticV1,
    fields: Fields,
}
impl CognitionHistoryRecordSchemaV1 {
    pub fn semantic(&self) -> &CognitionOriginalSemanticV1 {
        &self.semantic
    }
    pub fn scope(&self) -> CognitionHistoryRecordScopeV1 {
        self.fields.scope
    }
    pub fn identity_status(
        &self,
    ) -> crate::incremental_cognition_history::CognitionOriginalIdentityStatusV1 {
        self.semantic.identity_status()
    }
    pub fn causal_ref_count(&self) -> u64 {
        self.fields.causal_refs
    }
    pub fn event_kind(&self) -> Option<&str> {
        self.fields.kind.as_ref().map(Text::value)
    }
    pub fn journal_lifecycle(&self) -> Option<&CognitionJournalLifecycleProjectionV1> {
        self.fields.lifecycle.as_ref()
    }
}
pub(super) fn qualified(
    semantic: CognitionOriginalSemanticV1,
    fields: Fields,
) -> CognitionHistoryRecordSchemaV1 {
    CognitionHistoryRecordSchemaV1 { semantic, fields }
}
#[derive(Debug)]
pub(super) struct Fields {
    scope: CognitionHistoryRecordScopeV1,
    causal_refs: u64,
    kind: Option<Text>,
    lifecycle: Option<CognitionJournalLifecycleProjectionV1>,
}
#[derive(Debug, Clone, Copy)]
struct Text {
    bytes: [u8; 256],
    len: u16,
}
impl Text {
    fn new(value: &str) -> Result<Self, ValueTreeError> {
        if value.len() > 256 {
            return Err(bad());
        }
        let mut result = Self {
            bytes: [0; 256],
            len: value.len() as u16,
        };
        result.bytes[..value.len()].copy_from_slice(value.as_bytes());
        Ok(result)
    }
    fn value(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).expect("validated UTF8 metadata")
    }
}
#[derive(Clone, Copy)]
#[expect(
    clippy::large_enum_variant,
    reason = "Bounded scalar slots intentionally share one pre-admitted allocation."
)]
enum Scalar {
    Absent,
    Null,
    Present,
    Number(u64),
    Text(Text),
}
#[derive(Clone, Copy)]
enum Rule {
    Text,
    OptionalText,
    Bounded,
    Identity,
    Digest,
    Parent,
    OptionalDigest,
    U64,
    OptionalU64,
    Json,
    Request,
    Refs,
}
#[derive(Clone, Copy)]
struct Spec {
    name: &'static str,
    rule: Rule,
}
const fn s(name: &'static str, rule: Rule) -> Spec {
    Spec { name, rule }
}
use Rule::*;
const JOURNAL: &[Spec] = &[
    s("schema_version", Bounded),
    s("journal_seq", U64),
    s("parent_event_digest", Parent),
    s("event_kind", Bounded),
    s("kind", Bounded),
    s("world_id", Bounded),
    s("branch_id", Bounded),
    s("finality_epoch", U64),
    s("finality_block_hash", OptionalDigest),
    s("finality_status", Bounded),
    s("reorg_epoch", U64),
    s("logical_tick", U64),
    s("envelope_idempotency_key", Text),
    s("receipt_id", OptionalText),
    s("agent_id", OptionalText),
    s("agent_session_id", OptionalText),
    s("agent_turn_id", OptionalText),
    s("decision_request_id", OptionalText),
    s("request_digest", OptionalText),
    s("response_digest", OptionalText),
    s("envelope_digest", OptionalText),
    s("feedback_id", OptionalText),
    s("continuation_id", OptionalText),
    s("wake_id", OptionalText),
    s("scheduler_policy_digest", OptionalText),
    s("cursor_seq", OptionalU64),
    s("event_digest", Digest),
    s("retry_seq", U64),
    s("transport_attempt", U64),
    s("status", Bounded),
    s("payload_digest", Digest),
    s("causal_refs", Refs),
];
const ECONOMY: &[Spec] = &[
    s("schema_version", Bounded),
    s("journal_seq", U64),
    s("parent_event_digest", Parent),
    s("event_kind", Bounded),
    s("lease_id", Identity),
    s("operation_key", Identity),
    s("idempotency_key", Identity),
    s("resource", Identity),
    s("reserved_amount", U64),
    s("consumed_amount", U64),
    s("released_amount", U64),
    s("refunded_amount", U64),
    s("net_amount", U64),
    s("parent_receipt_id", OptionalText),
    s("reason", OptionalText),
    s("status", Bounded),
    s("receipt_id", Parent),
    s("event_digest", Digest),
];
const PROVISION: &[Spec] = &[
    s("schema_version", Bounded),
    s("journal_seq", U64),
    s("parent_event_digest", Parent),
    s("event_kind", Bounded),
    s("request", Request),
    s("receipt_id", Digest),
    s("event_digest", Digest),
];
const RESPONSE: &[Spec] = &[
    s("response_digest", Parent),
    s("response_artifact", Json),
    s("envelope_digest", Text),
    s("journal_head", Parent),
];
const REQUEST: &[Spec] = &[
    s("schema_version", Bounded),
    s("provision_id", Identity),
    s("account_id", Identity),
    s("owner_binding", Identity),
    s("owner_generation", U64),
    s("world_id", Identity),
    s("branch_id", Identity),
    s("reorg_epoch", U64),
    s("resource", Bounded),
    s("resource_version", Bounded),
    s("policy_revision", Bounded),
    s("allowance", U64),
    s("authority_context", Identity),
    s("authority_digest", Digest),
    s("provisioning_digest", Digest),
];
#[derive(Clone, Copy)]
struct Selected {
    group: u8,
    index: usize,
    spec: Spec,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    Ignore,
    Request,
    Refs,
}
pub(super) struct Schema {
    field: CognitionHistoryFieldV1,
    root: [Scalar; 32],
    request: [Scalar; 15],
    seen: u64,
    request_seen: u16,
    selected: [Option<Selected>; 3],
    context: [Context; 3],
    capture: Option<(usize, Selected)>,
    captured: [u8; 512],
    captured_len: usize,
    causal_refs: u64,
    projection: projection::Projection,
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition original local record fields")
}
impl Schema {
    pub(super) fn new(field: CognitionHistoryFieldV1) -> Self {
        Self {
            field,
            root: [Scalar::Absent; 32],
            request: [Scalar::Absent; 15],
            seen: 0,
            request_seen: 0,
            selected: [None; 3],
            context: [Context::Ignore; 3],
            capture: None,
            captured: [0; 512],
            captured_len: 0,
            causal_refs: 0,
            projection: projection::Projection::new(
                field == CognitionHistoryFieldV1::JournalEvents,
            ),
        }
    }
    fn specs(&self) -> &'static [Spec] {
        match self.field {
            CognitionHistoryFieldV1::JournalEvents => JOURNAL,
            CognitionHistoryFieldV1::EconomyJournal => ECONOMY,
            CognitionHistoryFieldV1::EconomyProvisionJournal => PROVISION,
            CognitionHistoryFieldV1::Responses => RESPONSE,
        }
    }
    pub(super) fn key(&mut self, depth: usize, key: &[u8]) -> Result<(), ValueTreeError> {
        if depth == 0 {
            self.projection.key(key)?;
        }
        let (group, specs) = if depth == 0 {
            (0, self.specs())
        } else if depth == 1 && self.context[1] == Context::Request {
            (1, REQUEST)
        } else {
            if depth == 1 {
                self.selected[2] = None;
            }
            return Ok(());
        };
        self.selected[depth + 1] = specs
            .iter()
            .enumerate()
            .find(|(_, s)| s.name.as_bytes() == key)
            .map(|(index, spec)| Selected {
                group,
                index,
                spec: *spec,
            });
        Ok(())
    }
    pub(super) fn begin(&mut self, depth: usize, initial: u8) -> Result<(), ValueTreeError> {
        self.projection.begin(depth, initial);
        if depth == 0 || depth > 2 {
            return Ok(());
        }
        self.context[depth] = Context::Ignore;
        if depth == 1 {
            self.selected[2] = None;
        }
        let selected = if depth == 2 && self.context[1] == Context::Refs {
            Some(Selected {
                group: 2,
                index: 0,
                spec: s("causal_ref", Bounded),
            })
        } else if depth == 1 || self.context[1] == Context::Request {
            self.selected[depth]
        } else {
            None
        };
        let Some(selected) = selected else {
            return Ok(());
        };
        let major = initial >> 5;
        let nullable = matches!(
            selected.spec.rule,
            OptionalText | OptionalDigest | OptionalU64
        );
        if initial == 0xf6 && nullable {
            self.store(selected, Scalar::Null);
            return Ok(());
        }
        let valid = match selected.spec.rule {
            Text | OptionalText | Bounded | Identity | Digest | Parent | OptionalDigest => {
                major == 3
            }
            U64 | OptionalU64 => major == 0,
            Request => major == 5,
            Refs => major == 4,
            Json => true,
        };
        if !valid {
            return Err(bad());
        }
        self.store(selected, Scalar::Present);
        match selected.spec.rule {
            Request => self.context[depth] = Context::Request,
            Refs => self.context[depth] = Context::Refs,
            Bounded | Identity | Digest | Parent | OptionalDigest | U64 | OptionalU64 => {
                self.capture = Some((depth, selected));
                self.captured[0] = initial;
                self.captured_len = 1;
            }
            _ => {}
        }
        Ok(())
    }
    fn store(&mut self, selected: Selected, value: Scalar) {
        match selected.group {
            0 => {
                self.root[selected.index] = value;
                self.seen |= 1 << selected.index;
            }
            1 => {
                self.request[selected.index] = value;
                self.request_seen |= 1 << selected.index;
            }
            _ => {}
        }
    }
    pub(super) fn feed(&mut self, bytes: &[u8]) -> Result<(), ValueTreeError> {
        self.projection.feed(bytes)?;
        if self.capture.is_some() {
            let end = self.captured_len.checked_add(bytes.len()).ok_or_else(bad)?;
            if end > self.captured.len() {
                return Err(ValueTreeError::ResourceLimited(
                    "Cognition local scalar capture",
                ));
            }
            self.captured[self.captured_len..end].copy_from_slice(bytes);
            self.captured_len = end;
        }
        Ok(())
    }
    pub(super) fn text(&mut self, depth: usize, length: u64) -> Result<(), ValueTreeError> {
        self.projection.text(depth, length);
        if self.capture.is_some_and(|(d, _)| d == depth) && length > 256 {
            return Err(bad());
        }
        Ok(())
    }
    pub(super) fn array(&mut self, depth: usize, length: u64) {
        if depth == 1 && self.context[1] == Context::Refs {
            self.causal_refs = length;
        }
    }
    pub(super) fn end(&mut self, depth: usize) -> Result<(), ValueTreeError> {
        self.projection.end(depth)?;
        let Some((d, selected)) = self.capture else {
            return Ok(());
        };
        if depth != d {
            return Ok(());
        }
        self.capture = None;
        let bytes = &self.captured[..self.captured_len];
        let scalar = if matches!(selected.spec.rule, U64 | OptionalU64) {
            Scalar::Number(serde_cbor::from_slice::<u64>(bytes).map_err(|_| bad())?)
        } else {
            let value: &str = serde_cbor::from_slice(bytes).map_err(|_| bad())?;
            match selected.spec.rule {
                Bounded | Identity => {
                    if value.trim().is_empty()
                        || value.len() > 256
                        || matches!(selected.spec.rule, Identity)
                            && value.chars().any(char::is_control)
                    {
                        return Err(bad());
                    }
                }
                Digest | OptionalDigest => {
                    if schema::identity(value)?.is_none() {
                        return Err(bad());
                    }
                }
                Parent => {
                    schema::identity(value)?;
                }
                _ => {}
            }
            Scalar::Text(Text::new(value)?)
        };
        self.store(selected, scalar);
        Ok(())
    }
    fn value(&self, name: &str) -> Result<&Scalar, ValueTreeError> {
        let index = self
            .specs()
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(bad)?;
        Ok(&self.root[index])
    }
    fn string(&self, name: &str) -> Result<&str, ValueTreeError> {
        match self.value(name)? {
            Scalar::Text(value) => Ok(value.value()),
            _ => Err(bad()),
        }
    }
    fn number(&self, name: &str) -> Result<u64, ValueTreeError> {
        match self.value(name)? {
            Scalar::Number(value) => Ok(*value),
            _ => Err(bad()),
        }
    }
    fn request_string(&self, name: &str) -> Result<&str, ValueTreeError> {
        let index = REQUEST
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(bad)?;
        match &self.request[index] {
            Scalar::Text(v) => Ok(v.value()),
            _ => Err(bad()),
        }
    }
    fn request_number(&self, name: &str) -> Result<u64, ValueTreeError> {
        let index = REQUEST
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(bad)?;
        match self.request[index] {
            Scalar::Number(v) => Ok(v),
            _ => Err(bad()),
        }
    }
    fn required(&self) -> Result<(), ValueTreeError> {
        for (index, spec) in self.specs().iter().enumerate() {
            let optional = match self.field {
                CognitionHistoryFieldV1::JournalEvents => matches!(
                    spec.name,
                    "envelope_idempotency_key"
                        | "receipt_id"
                        | "agent_id"
                        | "agent_session_id"
                        | "agent_turn_id"
                        | "decision_request_id"
                        | "feedback_id"
                        | "continuation_id"
                        | "wake_id"
                        | "scheduler_policy_digest"
                        | "cursor_seq"
                ),
                CognitionHistoryFieldV1::EconomyJournal => matches!(
                    spec.name,
                    "released_amount" | "net_amount" | "parent_receipt_id" | "reason"
                ),
                CognitionHistoryFieldV1::Responses => {
                    matches!(spec.name, "response_artifact" | "journal_head")
                }
                _ => false,
            };
            if !optional && self.seen & (1 << index) == 0 {
                return Err(bad());
            }
        }
        Ok(())
    }
    pub(super) fn finish(
        &self,
        semantic: &CognitionOriginalSemanticV1,
        work: &mut CognitionHistoryCborWorkV1,
    ) -> Result<Fields, ValueTreeError> {
        self.required()?;
        let scope = match self.field {
            CognitionHistoryFieldV1::JournalEvents => {
                if self.string("schema_version")? != "cognition-journal-event.v1"
                    || self.number("journal_seq")? == 0
                    || self.string("kind")? != self.string("event_kind")?
                {
                    return Err(bad());
                }
                let hash = match self.value("finality_block_hash")? {
                    Scalar::Null => None,
                    Scalar::Text(v) => Some(v.value()),
                    _ => return Err(bad()),
                };
                if !crate::cognition::finality_binding_is_legal(
                    self.string("finality_status")?,
                    hash,
                ) {
                    return Err(bad());
                }
                CognitionHistoryRecordScopeV1::LocalJournalFields
            }
            CognitionHistoryFieldV1::EconomyJournal => {
                if self.string("schema_version")? != "cognition-economy-event.v1"
                    || self.number("journal_seq")? == 0
                    || self.string("event_kind")? != "reserve"
                        && schema::identity(self.string("receipt_id")?)?.is_none()
                {
                    return Err(bad());
                }
                let kind = self.string("event_kind")?;
                let status = match kind {
                    "reserve" => "reserved",
                    "settle" => "settled",
                    "release" => "released",
                    "expire" => "expired",
                    "refund" => "refunded",
                    _ => return Err(bad()),
                };
                if self.string("status")? != status {
                    return Err(bad());
                }
                digest(
                    "oasis7.cognition.economy.operation.v1",
                    &(self.string("lease_id")?, kind),
                    self.string("operation_key")?,
                    work,
                )?;
                // This branch of the existing validator needs no external
                // lease/receipt: a receipt-less reserve carries no settlement.
                if kind == "reserve" && self.string("receipt_id")?.is_empty() {
                    for name in [
                        "consumed_amount",
                        "released_amount",
                        "refunded_amount",
                        "net_amount",
                    ] {
                        if !matches!(self.value(name)?, Scalar::Absent | Scalar::Number(0)) {
                            return Err(bad());
                        }
                    }
                    for name in ["parent_receipt_id", "reason"] {
                        if !matches!(self.value(name)?, Scalar::Absent | Scalar::Null) {
                            return Err(bad());
                        }
                    }
                }
                CognitionHistoryRecordScopeV1::LocalEconomyFields
            }
            CognitionHistoryFieldV1::EconomyProvisionJournal => {
                if self.string("schema_version")? != "cognition-provisioning-event.v1"
                    || self.number("journal_seq")? == 0
                    || (self.number("journal_seq")? > 1
                        && schema::identity(self.string("parent_event_digest")?)?.is_none())
                    || self.string("event_kind")? != "provision"
                {
                    return Err(bad());
                }
                self.provision(work)?;
                CognitionHistoryRecordScopeV1::LocalProvisionFields
            }
            CognitionHistoryFieldV1::Responses => {
                CognitionHistoryRecordScopeV1::ResponseWrapperFields
            }
        };
        if self.field!=CognitionHistoryFieldV1::Responses&& !matches!(semantic.identity_status(),crate::incremental_cognition_history::CognitionOriginalIdentityStatusV1::DigestMatched(_)){return Err(bad());}
        let kind = if self.field == CognitionHistoryFieldV1::Responses {
            None
        } else {
            Some(Text::new(self.string("event_kind")?)?)
        };
        Ok(Fields {
            scope,
            causal_refs: self.causal_refs,
            kind,
            lifecycle: if self.field == CognitionHistoryFieldV1::JournalEvents {
                Some(self.projection.finish(self.number("transport_attempt")?)?)
            } else {
                None
            },
        })
    }
    fn provision(&self, work: &mut CognitionHistoryCborWorkV1) -> Result<(), ValueTreeError> {
        if self.request_seen != (1 << REQUEST.len()) - 1 {
            return Err(bad());
        }
        let t = |name| self.request_string(name);
        let n = |name| self.request_number(name);
        if t("schema_version")? != "cognition-provisioning.v1"
            || n("owner_generation")? == 0
            || n("allowance")? == 0
            || t("account_id")? != t("owner_binding")?
            || t("resource")? != "cognition_units"
            || t("resource_version")? != "cognition_units.v1"
            || t("policy_revision")? != "fixed_unit_experimental"
        {
            return Err(bad());
        }
        digest(
            "oasis7.cognition.economy.provisioning-authority.v1",
            &(
                t("account_id")?,
                t("owner_binding")?,
                t("world_id")?,
                n("owner_generation")?,
                t("branch_id")?,
                t("resource")?,
                n("reorg_epoch")?,
                t("resource_version")?,
                t("policy_revision")?,
                t("authority_context")?,
                t("provision_id")?,
            ),
            t("authority_digest")?,
            work,
        )?;
        digest(
            "oasis7.cognition.economy.provisioning.v1",
            &(
                t("provision_id")?,
                t("account_id")?,
                t("owner_binding")?,
                n("owner_generation")?,
                t("world_id")?,
                t("branch_id")?,
                n("reorg_epoch")?,
                t("resource")?,
                t("resource_version")?,
                n("allowance")?,
                t("policy_revision")?,
                t("authority_digest")?,
            ),
            t("provisioning_digest")?,
            work,
        )?;
        digest(
            "oasis7.cognition.economy.provisioning-receipt-id.v1",
            &(t("provision_id")?, t("provisioning_digest")?),
            self.string("receipt_id")?,
            work,
        )
    }
}
fn digest<T: serde::Serialize>(
    domain: &str,
    payload: &T,
    expected: &str,
    work: &mut CognitionHistoryCborWorkV1,
) -> Result<(), ValueTreeError> {
    spend(&mut work.nodes_remaining, 1)?;
    // All tuple inputs are the fixed original bounded metadata above. The
    // pre-admitted 32KiB workspace covers this sole small canonical encoder.
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(domain, payload)).map_err(|_| bad())?;
    if bytes.len() > 32768 || schema::identity(expected)? != Some(*blake3::hash(&bytes).as_bytes())
    {
        return Err(bad());
    }
    Ok(())
}

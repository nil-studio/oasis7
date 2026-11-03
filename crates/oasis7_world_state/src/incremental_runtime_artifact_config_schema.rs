//! Closed source vocabulary shared by production and subsequent typed readers.
use super::*;
use crate::incremental_field_member::{MemberPayloadCodecV1 as Codec, MemberRecordError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Manifest,
    Module,
    Active,
    Limits,
    Security,
}
impl BodyKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Module => "module_record",
            Self::Active => "active",
            Self::Limits => "limits",
            Self::Security => "security",
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Spec {
    pub codec: Codec,
    pub nullable: bool,
    pub integer_max: u64,
    pub labels: &'static [&'static str],
}
impl Spec {
    fn new(codec: Codec) -> Self {
        Self {
            codec,
            nullable: false,
            integer_max: u64::MAX,
            labels: &[],
        }
    }
    fn optional(mut self) -> Self {
        self.nullable = true;
        self
    }
    fn bound(mut self, max: u64) -> Self {
        self.integer_max = max;
        self
    }
    fn labels(mut self, labels: &'static [&'static str]) -> Self {
        self.labels = labels;
        self
    }
}
fn invalid() -> MemberRecordError {
    MemberRecordError::Invalid("artifact config closed column")
}

/// Address shape is part of the schema: a nested Vec index can never be
/// interpreted as a map key, and a zero index cannot stand for an absent one.
pub(super) fn spec(
    kind: BodyKind,
    address: ArtifactConfigAddressV1<'_>,
) -> Result<Spec, MemberRecordError> {
    let ArtifactConfigAddressV1 {
        section,
        ordinal,
        inner_ordinal,
        member,
        column,
    } = address;
    let fixed = ordinal.is_none() && inner_ordinal.is_none() && member.is_none();
    let indexed = ordinal.is_some() && inner_ordinal.is_none() && member.is_none();
    let nested = ordinal.is_some() && inner_ordinal.is_some() && member.is_none();
    let keyed = ordinal.is_none() && inner_ordinal.is_none() && member.is_some();
    let text = Spec::new(Codec::Text);
    let uint = Spec::new(Codec::U64);
    let boolean = Spec::new(Codec::Bool);
    let root = Spec::new(Codec::RecordRoot);
    let unit = Spec::new(Codec::Unit);
    Ok(match (kind, section, column) {
        (BodyKind::Manifest, "manifest", "version") if fixed => uint,
        (BodyKind::Manifest, "manifest", "content") if fixed => root,
        (BodyKind::Active, "active", "value") if fixed => text,
        (
            BodyKind::Security,
            "security",
            "allow_builtin_manifest_fallback"
            | "allow_identity_hash_signature"
            | "allow_local_finality_signing"
            | "allow_runtime_source_compile",
        ) if fixed => boolean,
        (
            BodyKind::Module | BodyKind::Limits,
            "limits",
            "max_mem_bytes" | "max_gas" | "max_output_bytes",
        ) if fixed => uint,
        (
            BodyKind::Module | BodyKind::Limits,
            "limits",
            "max_call_rate" | "max_effects" | "max_emits",
        ) if fixed => uint.bound(u64::from(u32::MAX)),
        (
            BodyKind::Module,
            "manifest",
            "module_id" | "name" | "version" | "wasm_hash" | "interface_version",
        ) if fixed => text,
        (BodyKind::Module, "manifest", "kind") if fixed => text.labels(&["reducer", "pure"]),
        (BodyKind::Module, "manifest", "role") if fixed => {
            text.labels(&["rule", "domain", "gameplay", "body", "agent_internal"])
        }
        (BodyKind::Module, "manifest", "artifact_identity_present") if fixed => boolean,
        (
            BodyKind::Module,
            "exports"
            | "required_caps"
            | "abi_contract.policy_hooks"
            | "abi_contract.gameplay.game_modes",
            "length",
        ) if fixed => uint,
        (
            BodyKind::Module,
            "exports"
            | "required_caps"
            | "abi_contract.policy_hooks"
            | "abi_contract.gameplay.game_modes",
            "value",
        ) if indexed => text,
        (BodyKind::Module, "subscriptions", "length") if fixed => uint,
        (BodyKind::Module, "subscriptions", "marker") if indexed => unit,
        (BodyKind::Module, "subscriptions", "stage") if indexed => {
            text.optional()
                .labels(&["pre_action", "post_action", "post_event", "tick"])
        }
        (BodyKind::Module, "subscriptions", "filters_present") if indexed => boolean,
        (BodyKind::Module, "subscriptions", "filters") if indexed => root,
        (
            BodyKind::Module,
            "subscriptions.event_kinds" | "subscriptions.action_kinds",
            "length",
        ) if indexed => uint,
        (BodyKind::Module, "subscriptions.event_kinds" | "subscriptions.action_kinds", "value")
            if nested =>
        {
            text
        }
        (BodyKind::Module, "abi_contract", "abi_version") if fixed => {
            uint.optional().bound(u64::from(u32::MAX))
        }
        (BodyKind::Module, "abi_contract", "input_schema" | "output_schema") if fixed => {
            text.optional()
        }
        (BodyKind::Module, "abi_contract", "gameplay_present") if fixed => boolean,
        (BodyKind::Module, "abi_contract.cap_slots", "length") if fixed => uint,
        (BodyKind::Module, "abi_contract.cap_slots", "value") if keyed => text,
        (BodyKind::Module, "abi_contract.gameplay", "kind") if fixed => {
            text.labels(&["war", "governance", "crisis", "economic", "meta"])
        }
        (BodyKind::Module, "abi_contract.gameplay", "min_players") if fixed => {
            uint.bound(u64::from(u16::MAX))
        }
        (BodyKind::Module, "abi_contract.gameplay", "max_players") if fixed => {
            uint.optional().bound(u64::from(u16::MAX))
        }
        (BodyKind::Module, "abi_contract.declarations.commands", "length") if fixed => uint,
        (
            BodyKind::Module,
            "abi_contract.declarations.commands",
            "namespace" | "name" | "schema_hash",
        ) if indexed => text,
        (BodyKind::Module, "abi_contract.declarations.commands", "schema_version") if indexed => {
            uint.bound(u64::from(u32::MAX))
        }
        (BodyKind::Module, "abi_contract.declarations.commands", "max_payload_bytes")
            if indexed =>
        {
            uint
        }
        (
            BodyKind::Module,
            "artifact_identity",
            "source_hash"
            | "build_manifest_hash"
            | "signer_node_id"
            | "signature_scheme"
            | "artifact_signature",
        ) if fixed => text,
        (BodyKind::Module, "record", "registered_at") if fixed => uint,
        (BodyKind::Module, "record", "registered_by") if fixed => text,
        (BodyKind::Module, "record", "audit_event_id") if fixed => uint.optional(),
        _ => return Err(invalid()),
    })
}
pub(super) fn validate(
    kind: BodyKind,
    address: ArtifactConfigAddressV1<'_>,
    value: ArtifactConfigPayloadV1<'_>,
) -> Result<(), MemberRecordError> {
    let spec = spec(kind, address)?;
    let valid = match value {
        ArtifactConfigPayloadV1::Json(_) => spec.codec == Codec::RecordRoot,
        ArtifactConfigPayloadV1::Primitive(value) => match value {
            MemberPayloadRefV1::Unit => spec.codec == Codec::Unit || spec.nullable,
            MemberPayloadRefV1::U64(value) => spec.codec == Codec::U64 && value <= spec.integer_max,
            MemberPayloadRefV1::Text(value) => {
                spec.codec == Codec::Text
                    && (spec.labels.is_empty() || spec.labels.contains(&value))
            }
            MemberPayloadRefV1::Bool(_) => spec.codec == Codec::Bool,
            _ => false,
        },
    };
    if valid { Ok(()) } else { Err(invalid()) }
}

/// A single existing leaf may change without altering presence, cardinality or
/// parent relationships. Structural edits use the explicit full-body operation.
pub(super) fn point_supported(kind: BodyKind, address: ArtifactConfigAddressV1<'_>) -> bool {
    spec(kind, address).is_ok_and(|spec| {
        spec.codec != Codec::RecordRoot
            && spec.codec != Codec::Unit
            && address.column != "length"
            && !address.column.ends_with("_present")
    })
}

//! Borrowed Runtime configuration projection. This module alone grants no
//! registry, artifact availability, execution or World qualification.
use crate::incremental_field_member::MemberPayloadRefV1;
use oasis7_wasm_abi::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactConfigFieldV1 {
    Manifest,
    RegistryManifest,
    RegistryRegisteredAt,
    RegistryRegisteredBy,
    RegistryAuditEventId,
    RegistryActive,
    Limits,
    Security,
}
impl ArtifactConfigFieldV1 {
    pub const ALL: [Self; 8] = [
        Self::Manifest,
        Self::RegistryManifest,
        Self::RegistryRegisteredAt,
        Self::RegistryRegisteredBy,
        Self::RegistryAuditEventId,
        Self::RegistryActive,
        Self::Limits,
        Self::Security,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::RegistryManifest => "module_registry.records.manifest",
            Self::RegistryRegisteredAt => "module_registry.records.registered_at",
            Self::RegistryRegisteredBy => "module_registry.records.registered_by",
            Self::RegistryAuditEventId => "module_registry.records.audit_event_id",
            Self::RegistryActive => "module_registry.active",
            Self::Limits => "module_limits_max",
            Self::Security => "release_security_policy",
        }
    }
    pub fn descriptor(self) -> crate::incremental_projection::FieldDescriptor {
        use crate::incremental_projection::*;
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: self.name(),
            shape: match self {
                Self::Manifest | Self::Limits | Self::Security => FieldShape::Value,
                _ => FieldShape::Map,
            },
            destination: FieldDestination::State(
                oasis7_formal_protocol::incremental_state::StateNamespaceV1::Artifacts,
            ),
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(b"oasis7.iws.runtime-artifact-config.v1\0");
        hash.update(self.name().as_bytes());
        *hash.finalize().as_bytes()
    }
    fn body_kind(self) -> ArtifactConfigBodyKindV1 {
        match self {
            Self::Manifest => ArtifactConfigBodyKindV1::Manifest,
            Self::Limits => ArtifactConfigBodyKindV1::Limits,
            Self::Security => ArtifactConfigBodyKindV1::Security,
            Self::RegistryActive => ArtifactConfigBodyKindV1::Active,
            _ => ArtifactConfigBodyKindV1::Module,
        }
    }
    fn global(self) -> bool {
        matches!(self, Self::Manifest | Self::Limits | Self::Security)
    }
}
#[derive(Clone, Copy)]
pub enum ArtifactConfigRecordKeyV1<'a> {
    Global,
    Member(&'a str),
}

/// Runtime owns this DTO; its adapter must destructure the original policy.
/// These stored flags are data, not an authorization grant.
#[derive(Clone, Copy)]
pub struct ArtifactSecurityPolicyRefV1 {
    pub allow_builtin_manifest_fallback: bool,
    pub allow_identity_hash_signature: bool,
    pub allow_local_finality_signing: bool,
    pub allow_runtime_source_compile: bool,
}
pub struct ArtifactConfigSourceRefV1<'a> {
    pub manifest: &'a crate::Manifest,
    pub registry: &'a ModuleRegistry,
    pub limits: &'a ModuleLimits,
    pub security: ArtifactSecurityPolicyRefV1,
}

/// Internal relative address. Original map keys and Vec ordinals are retained;
/// the section/column vocabulary is chosen only by the exhaustive visitors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArtifactConfigAddressV1<'a> {
    pub section: &'static str,
    pub ordinal: Option<u64>,
    pub inner_ordinal: Option<u64>,
    pub member: Option<&'a str>,
    pub column: &'static str,
}
impl<'a> ArtifactConfigAddressV1<'a> {
    fn semantic(self) -> impl crate::incremental_semantic_key::SemanticKeyV1 + 'a {
        (
            (self.section, self.column),
            (
                (u64::from(self.ordinal.is_some()), self.ordinal.unwrap_or(0)),
                (
                    (
                        u64::from(self.inner_ordinal.is_some()),
                        self.inner_ordinal.unwrap_or(0),
                    ),
                    (u64::from(self.member.is_some()), self.member.unwrap_or("")),
                ),
            ),
        )
    }
}
#[derive(Clone, Copy)]
pub enum ArtifactConfigPayloadV1<'a> {
    Primitive(MemberPayloadRefV1<'a>),
    /// Only an actual JsonValue source, never an arbitrary Serialize DTO.
    Json(&'a serde_json::Value),
}
struct Visitor<'a, F> {
    emit: F,
    _source: std::marker::PhantomData<&'a ()>,
}
impl<'a, E, F> Visitor<'a, F>
where
    F: FnMut(ArtifactConfigAddressV1<'a>, ArtifactConfigPayloadV1<'a>) -> Result<(), E>,
{
    fn cell(
        &mut self,
        section: &'static str,
        ordinal: Option<u64>,
        member: Option<&'a str>,
        column: &'static str,
        payload: MemberPayloadRefV1<'a>,
    ) -> Result<(), E> {
        (self.emit)(
            ArtifactConfigAddressV1 {
                section,
                ordinal,
                inner_ordinal: None,
                member,
                column,
            },
            ArtifactConfigPayloadV1::Primitive(payload),
        )
    }
    fn fixed(
        &mut self,
        section: &'static str,
        column: &'static str,
        payload: MemberPayloadRefV1<'a>,
    ) -> Result<(), E> {
        self.cell(section, None, None, column, payload)
    }
    fn texts(&mut self, section: &'static str, values: &'a [String]) -> Result<(), E> {
        self.fixed(
            section,
            "length",
            MemberPayloadRefV1::U64(values.len() as u64),
        )?;
        for (ordinal, value) in values.iter().enumerate() {
            self.cell(
                section,
                Some(ordinal as u64),
                None,
                "value",
                MemberPayloadRefV1::Text(value),
            )?;
        }
        Ok(())
    }
    fn optional_text(
        &mut self,
        section: &'static str,
        column: &'static str,
        value: &'a Option<String>,
    ) -> Result<(), E> {
        self.fixed(
            section,
            column,
            value
                .as_deref()
                .map_or(MemberPayloadRefV1::Unit, MemberPayloadRefV1::Text),
        )
    }
    fn limits(&mut self, section: &'static str, limits: &'a ModuleLimits) -> Result<(), E> {
        let ModuleLimits {
            max_mem_bytes,
            max_gas,
            max_call_rate,
            max_output_bytes,
            max_effects,
            max_emits,
        } = limits;
        for (name, value) in [
            ("max_mem_bytes", *max_mem_bytes),
            ("max_gas", *max_gas),
            ("max_call_rate", u64::from(*max_call_rate)),
            ("max_output_bytes", *max_output_bytes),
            ("max_effects", u64::from(*max_effects)),
            ("max_emits", u64::from(*max_emits)),
        ] {
            self.fixed(section, name, MemberPayloadRefV1::U64(value))?;
        }
        Ok(())
    }
}

/// Exhaustive source projection; callbacks own admission/work/cancellation.
/// No references to temporary serializer values are retained.
pub fn visit_artifact_module_manifest_v1<'a, E>(
    source: &'a ModuleManifest,
    emit: impl FnMut(ArtifactConfigAddressV1<'a>, ArtifactConfigPayloadV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use MemberPayloadRefV1 as P;
    let ModuleManifest {
        module_id,
        name,
        version,
        kind,
        role,
        wasm_hash,
        interface_version,
        exports,
        subscriptions,
        required_caps,
        abi_contract,
        artifact_identity,
        limits,
    } = source;
    let mut visitor = Visitor {
        emit,
        _source: std::marker::PhantomData,
    };
    for (column, value) in [
        ("module_id", module_id.as_str()),
        ("name", name),
        ("version", version),
        ("wasm_hash", wasm_hash),
        ("interface_version", interface_version),
    ] {
        visitor.fixed("manifest", column, P::Text(value))?;
    }
    visitor.fixed(
        "manifest",
        "kind",
        P::Text(match kind {
            ModuleKind::Reducer => "reducer",
            ModuleKind::Pure => "pure",
        }),
    )?;
    visitor.fixed(
        "manifest",
        "role",
        P::Text(match role {
            ModuleRole::Rule => "rule",
            ModuleRole::Domain => "domain",
            ModuleRole::Gameplay => "gameplay",
            ModuleRole::Body => "body",
            ModuleRole::AgentInternal => "agent_internal",
        }),
    )?;
    visitor.texts("exports", exports)?;
    visitor.texts("required_caps", required_caps)?;
    visitor.limits("limits", limits)?;
    visitor.fixed(
        "subscriptions",
        "length",
        P::U64(subscriptions.len() as u64),
    )?;
    for (index, subscription) in subscriptions.iter().enumerate() {
        let ordinal = Some(index as u64);
        let ModuleSubscription {
            event_kinds,
            action_kinds,
            stage,
            filters,
        } = subscription;
        visitor.cell("subscriptions", ordinal, None, "marker", P::Unit)?;
        visitor.cell(
            "subscriptions",
            ordinal,
            None,
            "stage",
            stage.map_or(P::Unit, |stage| {
                P::Text(match stage {
                    ModuleSubscriptionStage::PreAction => "pre_action",
                    ModuleSubscriptionStage::PostAction => "post_action",
                    ModuleSubscriptionStage::PostEvent => "post_event",
                    ModuleSubscriptionStage::Tick => "tick",
                })
            }),
        )?;
        for (section, values) in [
            ("subscriptions.event_kinds", event_kinds),
            ("subscriptions.action_kinds", action_kinds),
        ] {
            visitor.cell(
                section,
                ordinal,
                None,
                "length",
                P::U64(values.len() as u64),
            )?;
            // The inner ordinal is a U64 semantic-key component, not text.
            for (inner, value) in values.iter().enumerate() {
                (visitor.emit)(
                    ArtifactConfigAddressV1 {
                        section,
                        ordinal,
                        inner_ordinal: Some(inner as u64),
                        member: None,
                        column: "value",
                    },
                    ArtifactConfigPayloadV1::Primitive(P::Text(value)),
                )?;
            }
        }
        visitor.cell(
            "subscriptions",
            ordinal,
            None,
            "filters_present",
            P::Bool(filters.is_some()),
        )?;
        if let Some(filters) = filters {
            (visitor.emit)(
                ArtifactConfigAddressV1 {
                    section: "subscriptions",
                    ordinal,
                    inner_ordinal: None,
                    member: None,
                    column: "filters",
                },
                ArtifactConfigPayloadV1::Json(filters),
            )?;
        }
    }
    let ModuleAbiContract {
        abi_version,
        input_schema,
        output_schema,
        cap_slots,
        policy_hooks,
        gameplay,
        declarations,
    } = abi_contract;
    visitor.fixed(
        "abi_contract",
        "abi_version",
        abi_version.map_or(P::Unit, |value| P::U64(u64::from(value))),
    )?;
    visitor.optional_text("abi_contract", "input_schema", input_schema)?;
    visitor.optional_text("abi_contract", "output_schema", output_schema)?;
    visitor.fixed(
        "abi_contract.cap_slots",
        "length",
        P::U64(cap_slots.len() as u64),
    )?;
    for (key, value) in cap_slots {
        visitor.cell(
            "abi_contract.cap_slots",
            None,
            Some(key),
            "value",
            P::Text(value),
        )?;
    }
    visitor.texts("abi_contract.policy_hooks", policy_hooks)?;
    visitor.fixed(
        "abi_contract",
        "gameplay_present",
        P::Bool(gameplay.is_some()),
    )?;
    if let Some(gameplay) = gameplay {
        let GameplayContract {
            kind,
            game_modes,
            min_players,
            max_players,
        } = gameplay;
        visitor.fixed("abi_contract.gameplay", "kind", P::Text(kind.as_str()))?;
        visitor.fixed(
            "abi_contract.gameplay",
            "min_players",
            P::U64(u64::from(*min_players)),
        )?;
        visitor.fixed(
            "abi_contract.gameplay",
            "max_players",
            max_players.map_or(P::Unit, |value| P::U64(u64::from(value))),
        )?;
        visitor.texts("abi_contract.gameplay.game_modes", game_modes)?;
    }
    let ModuleSchemaDeclarations { commands } = declarations;
    visitor.fixed(
        "abi_contract.declarations.commands",
        "length",
        P::U64(commands.len() as u64),
    )?;
    for (index, command) in commands.iter().enumerate() {
        let ModuleCommandDeclaration {
            namespace,
            name,
            schema_version,
            schema_hash,
            max_payload_bytes,
        } = command;
        let ordinal = Some(index as u64);
        for (column, value) in [
            ("namespace", namespace),
            ("name", name),
            ("schema_hash", schema_hash),
        ] {
            visitor.cell(
                "abi_contract.declarations.commands",
                ordinal,
                None,
                column,
                P::Text(value),
            )?;
        }
        visitor.cell(
            "abi_contract.declarations.commands",
            ordinal,
            None,
            "schema_version",
            P::U64(u64::from(*schema_version)),
        )?;
        visitor.cell(
            "abi_contract.declarations.commands",
            ordinal,
            None,
            "max_payload_bytes",
            P::U64(*max_payload_bytes),
        )?;
    }
    visitor.fixed(
        "manifest",
        "artifact_identity_present",
        P::Bool(artifact_identity.is_some()),
    )?;
    if let Some(identity) = artifact_identity {
        let ModuleArtifactIdentity {
            source_hash,
            build_manifest_hash,
            signer_node_id,
            signature_scheme,
            artifact_signature,
        } = identity;
        for (column, value) in [
            ("source_hash", source_hash),
            ("build_manifest_hash", build_manifest_hash),
            ("signer_node_id", signer_node_id),
            ("signature_scheme", signature_scheme),
            ("artifact_signature", artifact_signature),
        ] {
            visitor.fixed("artifact_identity", column, P::Text(value))?;
        }
    }
    Ok(())
}

pub fn visit_artifact_module_limits_v1<'a, E>(
    source: &'a ModuleLimits,
    emit: impl FnMut(ArtifactConfigAddressV1<'a>, ArtifactConfigPayloadV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    Visitor {
        emit,
        _source: std::marker::PhantomData,
    }
    .limits("limits", source)
}

/// The original registry map key is supplied separately by the owner. Neither
/// this visitor nor a reader derives it from module_id/version.
pub fn visit_artifact_module_record_v1<'a, E>(
    source: &'a ModuleRecord,
    mut emit: impl FnMut(ArtifactConfigAddressV1<'a>, ArtifactConfigPayloadV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let ModuleRecord {
        manifest,
        registered_at,
        registered_by,
        audit_event_id,
    } = source;
    visit_artifact_module_manifest_v1(manifest, &mut emit)?;
    let mut visitor = Visitor {
        emit,
        _source: std::marker::PhantomData,
    };
    visitor.fixed(
        "record",
        "registered_at",
        MemberPayloadRefV1::U64(*registered_at),
    )?;
    visitor.fixed(
        "record",
        "registered_by",
        MemberPayloadRefV1::Text(registered_by),
    )?;
    visitor.fixed(
        "record",
        "audit_event_id",
        audit_event_id.map_or(MemberPayloadRefV1::Unit, MemberPayloadRefV1::U64),
    )
}

pub fn visit_artifact_world_manifest_v1<'a, E>(
    source: &'a crate::Manifest,
    mut emit: impl FnMut(ArtifactConfigAddressV1<'a>, ArtifactConfigPayloadV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let crate::Manifest { version, content } = source;
    emit(
        ArtifactConfigAddressV1 {
            section: "manifest",
            ordinal: None,
            inner_ordinal: None,
            member: None,
            column: "version",
        },
        ArtifactConfigPayloadV1::Primitive(MemberPayloadRefV1::U64(*version)),
    )?;
    emit(
        ArtifactConfigAddressV1 {
            section: "manifest",
            ordinal: None,
            inner_ordinal: None,
            member: None,
            column: "content",
        },
        ArtifactConfigPayloadV1::Json(content),
    )
}

pub fn visit_artifact_security_policy_v1<'a, E>(
    source: ArtifactSecurityPolicyRefV1,
    emit: impl FnMut(ArtifactConfigAddressV1<'a>, ArtifactConfigPayloadV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let ArtifactSecurityPolicyRefV1 {
        allow_builtin_manifest_fallback,
        allow_identity_hash_signature,
        allow_local_finality_signing,
        allow_runtime_source_compile,
    } = source;
    let mut visitor = Visitor {
        emit,
        _source: std::marker::PhantomData,
    };
    for (column, value) in [
        (
            "allow_builtin_manifest_fallback",
            allow_builtin_manifest_fallback,
        ),
        (
            "allow_identity_hash_signature",
            allow_identity_hash_signature,
        ),
        ("allow_local_finality_signing", allow_local_finality_signing),
        ("allow_runtime_source_compile", allow_runtime_source_compile),
    ] {
        visitor.fixed("security", column, MemberPayloadRefV1::Bool(value))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "incremental_runtime_artifact_config_body_tests.rs"]
mod body_tests;
#[cfg(test)]
#[path = "incremental_runtime_artifact_config_fault_tests.rs"]
mod fault_tests;
#[cfg(test)]
#[path = "incremental_runtime_artifact_config_operations_tests.rs"]
mod operations_tests;
#[cfg(test)]
#[path = "incremental_runtime_artifact_config_pages_tests.rs"]
mod pages_tests;
#[cfg(test)]
#[path = "incremental_runtime_artifact_config_tests.rs"]
mod tests;

#[path = "incremental_runtime_artifact_config_genesis.rs"]
mod genesis;
pub use genesis::Body as ArtifactConfigBodyRefV1;
pub use genesis::{ArtifactConfigWriteWorkV1, prepare_artifact_config_genesis_v1};
#[path = "incremental_runtime_artifact_config_schema.rs"]
mod schema;
pub use schema::BodyKind as ArtifactConfigBodyKindV1;
#[path = "incremental_runtime_artifact_config_read.rs"]
mod read;
pub use read::{ArtifactConfigValueV1, inspect_artifact_config_cell_v1};
#[path = "incremental_runtime_artifact_config_closure.rs"]
mod closure;
pub use closure::{
    ArtifactConfigBodyQualificationV1, ArtifactConfigReadWorkV1, qualify_artifact_config_body_v1,
    readmit_artifact_config_body_ordinary_v1,
};
#[path = "incremental_runtime_artifact_config_family.rs"]
mod family;
pub use family::{
    ArtifactConfigFamilyQualificationV1, open_qualified_artifact_config_body_v1,
    qualify_artifact_config_family_v1, readmit_artifact_config_family_ordinary_v1,
};
#[path = "incremental_runtime_artifact_config_overlay.rs"]
mod overlay;
pub use overlay::{
    ArtifactConfigCellChangeV1, ArtifactConfigCellRequestV1, ArtifactConfigScalarRefV1,
    change_qualified_artifact_config_cell_v1, read_qualified_artifact_config_overlay_v1,
};
#[path = "incremental_runtime_artifact_config_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::{
    ArtifactConfigRecordChangeResultV1, ArtifactConfigRecordChangeV1,
    change_qualified_artifact_config_record_v1,
};
#[path = "incremental_runtime_artifact_config_key_read.rs"]
mod key_read;
#[path = "incremental_runtime_artifact_config_receiver.rs"]
mod receiver;
pub use receiver::{
    accept_artifact_config_transition_v1, accept_artifact_config_transition_with_json_v1,
};
#[path = "incremental_runtime_artifact_config_pages.rs"]
mod pages;
pub use pages::{
    ArtifactConfigPagesV1, ArtifactConfigPagesWorkV1, mark_artifact_config_pages_v1,
    transfer_artifact_config_pages_v1,
};
#[path = "incremental_runtime_artifact_config_json.rs"]
mod json;
pub use json::{
    ArtifactConfigJsonChangeResultV1, ArtifactConfigJsonChangeV1, ArtifactConfigJsonParentsV1,
    ArtifactConfigJsonQualificationV1, change_qualified_artifact_config_json_v1,
    materialize_artifact_config_json_v1, open_artifact_config_json_v1,
};
#[path = "incremental_runtime_artifact_config_selected.rs"]
mod selected;
pub use selected::{
    ArtifactConfigSelectedBodyRequestV1, open_artifact_config_body_overlay_v1,
    require_empty_artifact_registry_overlay_v1,
};
#[path = "incremental_runtime_artifact_config_module_read.rs"]
mod module_read;
pub use module_read::{
    SelectedModuleManifestLimitsV1, SelectedModuleManifestV1, read_selected_module_manifest_v1,
};

pub(crate) use receiver::accept_artifact_config_verified_v1;

#[path = "incremental_runtime_artifact_config_current_keys.rs"]
mod current_keys;
pub use current_keys::read_current_registry_active_id_v1;

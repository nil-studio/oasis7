//! Versioned capability authorization wire types and pure ABI helpers.
//!
//! This module deliberately does not authenticate signatures or consult a
//! catalog/provider as an authority. Those checks belong to the trusted host.

use serde::{Deserialize, Serialize};
use std::fmt;

use super::{ModuleCommandEnvelope, encode_canonical_cbor};

pub const CAPABILITY_GRANT_VERSION_V2: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilitySubject {
    Agent {
        agent_id: String,
        owner_binding: String,
        generation: u64,
    },
    Module {
        module_id: String,
        module_version: String,
        instance_id: String,
    },
    System {
        system_id: String,
        epoch: u64,
    },
}

impl CapabilitySubject {
    pub fn validate(&self) -> Result<(), CapabilityAuthorizationValidationError> {
        self.validate_with_work_v1(&mut |_, _| Ok(()))
    }
    pub fn validate_with_fallible_work_v1<E>(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Result<(), CapabilityAuthorizationValidationError>, E> {
        let mut first = None;
        let result = self.validate_with_work_v1(&mut |s, b| match work(s, b) {
            Ok(()) => Ok(()),
            Err(e) => {
                if first.is_none() {
                    first = Some(e)
                }
                Err(CapabilityAuthorizationValidationError::EmptyField(
                    "bounded_work",
                ))
            }
        });
        match first {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }
    pub fn validate_with_work_v1(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
    ) -> Result<(), CapabilityAuthorizationValidationError> {
        work(1, 0)?;
        match self {
            Self::Agent {
                agent_id,
                owner_binding,
                ..
            } => {
                required_with_work_v1("subject.agent_id", agent_id, work)?;
                required_with_work_v1("subject.owner_binding", owner_binding, work)?;
            }
            Self::Module {
                module_id,
                module_version,
                instance_id,
            } => {
                required_with_work_v1("subject.module_id", module_id, work)?;
                required_with_work_v1("subject.module_version", module_version, work)?;
                required_with_work_v1("subject.instance_id", instance_id, work)?;
            }
            Self::System { system_id, .. } => {
                required_with_work_v1("subject.system_id", system_id, work)?
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityPresenter {
    pub presenter_id: String,
    pub presenter_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation_ref: Option<String>,
}

impl CapabilityPresenter {
    pub fn validate(&self) -> Result<(), CapabilityAuthorizationValidationError> {
        self.validate_with_work_v1(&mut |_, _| Ok(()))
    }
    pub fn validate_with_fallible_work_v1<E>(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Result<(), CapabilityAuthorizationValidationError>, E> {
        let mut first = None;
        let result = self.validate_with_work_v1(&mut |s, b| match work(s, b) {
            Ok(()) => Ok(()),
            Err(e) => {
                if first.is_none() {
                    first = Some(e)
                }
                Err(CapabilityAuthorizationValidationError::EmptyField(
                    "bounded_work",
                ))
            }
        });
        match first {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }
    pub fn validate_with_work_v1(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
    ) -> Result<(), CapabilityAuthorizationValidationError> {
        work(1, 0)?;
        required_with_work_v1("presenter.presenter_id", &self.presenter_id, work)?;
        if !matches!(
            self.presenter_kind.as_str(),
            "provider" | "agent_client" | "module" | "viewer"
        ) {
            return Err(CapabilityAuthorizationValidationError::InvalidEnum {
                field: "presenter.presenter_kind",
                value: self.presenter_kind.clone(),
            });
        }
        optional_required_with_work_v1("presenter.session_id", self.session_id.as_deref(), work)?;
        optional_required_with_work_v1(
            "presenter.attestation_ref",
            self.attestation_ref.as_deref(),
            work,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityAudience {
    pub world_id: String,
    pub branch_id: String,
    pub finality_epoch: u64,
    pub target_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
}

impl CapabilityAudience {
    pub fn validate(&self) -> Result<(), CapabilityAuthorizationValidationError> {
        self.validate_with_work_v1(&mut |_, _| Ok(()))
    }
    pub fn validate_with_fallible_work_v1<E>(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Result<(), CapabilityAuthorizationValidationError>, E> {
        let mut first = None;
        let result = self.validate_with_work_v1(&mut |s, b| match work(s, b) {
            Ok(()) => Ok(()),
            Err(e) => {
                if first.is_none() {
                    first = Some(e)
                }
                Err(CapabilityAuthorizationValidationError::EmptyField(
                    "bounded_work",
                ))
            }
        });
        match first {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }
    pub fn validate_with_work_v1(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
    ) -> Result<(), CapabilityAuthorizationValidationError> {
        work(1, 0)?;
        required_with_work_v1("audience.world_id", &self.world_id, work)?;
        required_with_work_v1("audience.branch_id", &self.branch_id, work)?;
        if !matches!(
            self.target_kind.as_str(),
            "world" | "institution" | "module_instance"
        ) {
            return Err(CapabilityAuthorizationValidationError::InvalidEnum {
                field: "audience.target_kind",
                value: self.target_kind.clone(),
            });
        }
        optional_required_with_work_v1("audience.target_id", self.target_id.as_deref(), work)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityIssuer {
    pub issuer_id: String,
    pub issuer_kind: String,
    pub governance_epoch: u64,
    pub finalized_receipt_id: String,
    pub key_id: String,
    pub issuer_key_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_rotation_receipt_id: Option<String>,
    pub signature: String,
}

impl CapabilityIssuer {
    pub fn validate(&self) -> Result<(), CapabilityAuthorizationValidationError> {
        self.validate_with_work_v1(&mut |_, _| Ok(()))
    }
    pub fn validate_with_fallible_work_v1<E>(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Result<(), CapabilityAuthorizationValidationError>, E> {
        let mut first = None;
        let result = self.validate_with_work_v1(&mut |s, b| match work(s, b) {
            Ok(()) => Ok(()),
            Err(e) => {
                if first.is_none() {
                    first = Some(e)
                }
                Err(CapabilityAuthorizationValidationError::EmptyField(
                    "bounded_work",
                ))
            }
        });
        match first {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }
    pub fn validate_with_work_v1(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
    ) -> Result<(), CapabilityAuthorizationValidationError> {
        work(1, 0)?;
        required_with_work_v1("issuer.issuer_id", &self.issuer_id, work)?;
        if !matches!(
            self.issuer_kind.as_str(),
            "governance" | "kernel_migration" | "system"
        ) {
            return Err(CapabilityAuthorizationValidationError::InvalidEnum {
                field: "issuer.issuer_kind",
                value: self.issuer_kind.clone(),
            });
        }
        required_with_work_v1(
            "issuer.finalized_receipt_id",
            &self.finalized_receipt_id,
            work,
        )?;
        required_with_work_v1("issuer.key_id", &self.key_id, work)?;
        required_with_work_v1("issuer.signature", &self.signature, work)?;
        optional_required_with_work_v1(
            "issuer.authority_rotation_receipt_id",
            self.authority_rotation_receipt_id.as_deref(),
            work,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityScope {
    pub module_id: String,
    pub module_version: String,
    pub namespace: String,
    pub object_kind: String,
    pub object_name: String,
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_selector: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_selector: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_payload_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_class: Option<String>,
}

impl CapabilityScope {
    pub fn validate(&self) -> Result<(), CapabilityAuthorizationValidationError> {
        self.validate_with_work_v1(&mut |_, _| Ok(()))
    }
    pub fn validate_with_fallible_work_v1<E>(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Result<(), CapabilityAuthorizationValidationError>, E> {
        let mut first = None;
        let result = self.validate_with_work_v1(&mut |s, b| match work(s, b) {
            Ok(()) => Ok(()),
            Err(e) => {
                if first.is_none() {
                    first = Some(e)
                }
                Err(CapabilityAuthorizationValidationError::EmptyField(
                    "bounded_work",
                ))
            }
        });
        match first {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }
    pub fn validate_with_work_v1(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
    ) -> Result<(), CapabilityAuthorizationValidationError> {
        work(1, 0)?;
        for (field, value) in [
            ("scope.module_id", self.module_id.as_str()),
            ("scope.module_version", self.module_version.as_str()),
            ("scope.namespace", self.namespace.as_str()),
            ("scope.object_kind", self.object_kind.as_str()),
            ("scope.object_name", self.object_name.as_str()),
            ("scope.operation", self.operation.as_str()),
        ] {
            required_with_work_v1(field, value, work)?;
        }
        if !matches!(
            self.object_kind.as_str(),
            "command" | "component" | "event" | "effect"
        ) {
            return Err(CapabilityAuthorizationValidationError::InvalidEnum {
                field: "scope.object_kind",
                value: self.object_kind.clone(),
            });
        }
        if !matches!(
            self.operation.as_str(),
            "execute" | "read" | "write" | "emit" | "invoke"
        ) {
            return Err(CapabilityAuthorizationValidationError::InvalidEnum {
                field: "scope.operation",
                value: self.operation.clone(),
            });
        }
        validate_selectors_with_work_v1(
            "scope.entity_selector",
            self.entity_selector.as_deref(),
            work,
        )?;
        validate_selectors_with_work_v1(
            "scope.resource_selector",
            self.resource_selector.as_deref(),
            work,
        )?;
        if self.max_payload_bytes == Some(0) {
            return Err(CapabilityAuthorizationValidationError::InvalidBound {
                field: "scope.max_payload_bytes",
                value: 0,
            });
        }
        optional_required_with_work_v1("scope.policy_class", self.policy_class.as_deref(), work)?;
        Ok(())
    }

    pub fn matches_exact(&self, requested: &Self) -> bool {
        self.validate().is_ok()
            && requested.validate().is_ok()
            && self.identity_matches(requested)
            && self.entity_selector == requested.entity_selector
            && self.resource_selector == requested.resource_selector
            && self.max_payload_bytes == requested.max_payload_bytes
            && self.policy_class == requested.policy_class
    }

    pub fn allows(&self, requested: &Self) -> bool {
        self.validate().is_ok()
            && requested.validate().is_ok()
            && self.identity_matches(requested)
            && self.entity_selector == requested.entity_selector
            && self.resource_selector == requested.resource_selector
            && bound_contains(self.max_payload_bytes, requested.max_payload_bytes)
            && self.policy_class == requested.policy_class
    }

    pub fn contains_subset(&self, child: &Self) -> bool {
        match self.contains_subset_with_fallible_work_v1(child, &mut |_, _| {
            Ok::<_, std::convert::Infallible>(())
        }) {
            Ok(v) => v,
            Err(e) => match e {},
        }
    }
    pub fn contains_subset_with_fallible_work_v1<E>(
        &self,
        child: &Self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<bool, E> {
        Ok(self.validate_with_fallible_work_v1(work)?.is_ok()
            && child.validate_with_fallible_work_v1(work)?.is_ok()
            && self.module_id == child.module_id
            && self.module_version == child.module_version
            && self.namespace == child.namespace
            && self.object_kind == child.object_kind
            && self.object_name == child.object_name
            && operation_contains(&self.operation, &child.operation)
            && selectors_contain_with_work_v1(
                self.entity_selector.as_deref(),
                child.entity_selector.as_deref(),
                work,
            )?
            && selectors_contain_with_work_v1(
                self.resource_selector.as_deref(),
                child.resource_selector.as_deref(),
                work,
            )?
            && bound_contains(self.max_payload_bytes, child.max_payload_bytes)
            && option_contains(self.policy_class.as_deref(), child.policy_class.as_deref()))
    }

    pub fn is_subset_of(&self, parent: &Self) -> bool {
        parent.contains_subset(self)
    }

    fn identity_matches(&self, other: &Self) -> bool {
        self.module_id == other.module_id
            && self.module_version == other.module_version
            && self.namespace == other.namespace
            && self.object_kind == other.object_kind
            && self.object_name == other.object_name
            && self.operation == other.operation
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityGrantV2 {
    pub grant_id: String,
    pub grant_version: u32,
    pub subject: CapabilitySubject,
    pub audience: CapabilityAudience,
    pub issuer: CapabilityIssuer,
    pub scope: CapabilityScope,
    pub issued_at_tick: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_tick: Option<u64>,
    pub grant_nonce: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_grant_id: Option<String>,
    pub delegation_depth: u32,
    pub revocation_epoch: u64,
    pub status: String,
    pub canonical_body_hash: String,
    pub issuance_signature: String,
}

impl CapabilityGrantV2 {
    pub fn validate(&self) -> Result<(), CapabilityAuthorizationValidationError> {
        self.validate_with_work_v1(&mut |_, _| Ok(()))
    }
    pub fn validate_with_fallible_work_v1<E>(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Result<(), CapabilityAuthorizationValidationError>, E> {
        let mut first = None;
        let result = self.validate_with_work_v1(&mut |s, b| match work(s, b) {
            Ok(()) => Ok(()),
            Err(e) => {
                if first.is_none() {
                    first = Some(e)
                }
                Err(CapabilityAuthorizationValidationError::EmptyField(
                    "bounded_work",
                ))
            }
        });
        match first {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }
    pub fn validate_with_work_v1(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
    ) -> Result<(), CapabilityAuthorizationValidationError> {
        work(1, 0)?;
        if self.grant_version != CAPABILITY_GRANT_VERSION_V2 {
            return Err(CapabilityAuthorizationValidationError::UnsupportedVersion(
                self.grant_version,
            ));
        }
        required_with_work_v1("grant.grant_id", &self.grant_id, work)?;
        self.subject.validate_with_work_v1(work)?;
        self.audience.validate_with_work_v1(work)?;
        self.issuer.validate_with_work_v1(work)?;
        self.scope.validate_with_work_v1(work)?;
        required_with_work_v1("grant.grant_nonce", &self.grant_nonce, work)?;
        required_with_work_v1("grant.status", &self.status, work)?;
        required_with_work_v1("grant.canonical_body_hash", &self.canonical_body_hash, work)?;
        required_with_work_v1("grant.issuance_signature", &self.issuance_signature, work)?;
        optional_required_with_work_v1(
            "grant.parent_grant_id",
            self.parent_grant_id.as_deref(),
            work,
        )?;
        if let Some(expires_at_tick) = self.expires_at_tick
            && expires_at_tick < self.issued_at_tick
        {
            return Err(CapabilityAuthorizationValidationError::InvalidLifetime {
                issued_at_tick: self.issued_at_tick,
                expires_at_tick,
            });
        }
        Ok(())
    }

    /// Original body preimage; borrowed DATA, never a grant qualification.
    pub fn canonical_body_input_v1(&self) -> impl Serialize + '_ {
        CapabilityGrantV2Body {
            grant_version: self.grant_version,
            subject: &self.subject,
            audience: &self.audience,
            issuer: CapabilityIssuerBody::from(&self.issuer),
            scope: &self.scope,
            issued_at_tick: self.issued_at_tick,
            expires_at_tick: self.expires_at_tick,
            grant_nonce: &self.grant_nonce,
            parent_grant_id: self.parent_grant_id.as_deref(),
            delegation_depth: self.delegation_depth,
            revocation_epoch: self.revocation_epoch,
            status: &self.status,
        }
    }

    pub fn canonical_body_bytes(&self) -> Result<Vec<u8>, CapabilityAuthorizationValidationError> {
        let body = self.canonical_body_input_v1();
        encode_canonical_cbor(&body).map_err(|error| {
            CapabilityAuthorizationValidationError::CanonicalEncoding(error.to_string())
        })
    }

    pub fn canonical_body_hash(&self) -> Result<String, CapabilityAuthorizationValidationError> {
        canonical_sha256_hex(&self.canonical_body_bytes()?)
    }

    pub fn body_hash_matches(&self) -> Result<bool, CapabilityAuthorizationValidationError> {
        Ok(self.canonical_body_hash()? == self.canonical_body_hash)
    }

    pub fn expected_grant_id(&self) -> Result<String, CapabilityAuthorizationValidationError> {
        self.canonical_body_hash()
    }

    pub fn grant_id_matches_body(&self) -> Result<bool, CapabilityAuthorizationValidationError> {
        Ok(self.expected_grant_id()? == self.grant_id)
    }
}

#[derive(Debug, Serialize)]
struct CapabilityGrantV2Body<'a> {
    grant_version: u32,
    subject: &'a CapabilitySubject,
    audience: &'a CapabilityAudience,
    issuer: CapabilityIssuerBody<'a>,
    scope: &'a CapabilityScope,
    issued_at_tick: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at_tick: Option<u64>,
    grant_nonce: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_grant_id: Option<&'a str>,
    delegation_depth: u32,
    revocation_epoch: u64,
    status: &'a str,
}

#[derive(Debug, Serialize)]
struct CapabilityIssuerBody<'a> {
    issuer_id: &'a str,
    issuer_kind: &'a str,
    governance_epoch: u64,
    finalized_receipt_id: &'a str,
    key_id: &'a str,
    issuer_key_epoch: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    authority_rotation_receipt_id: Option<&'a str>,
}

impl<'a> From<&'a CapabilityIssuer> for CapabilityIssuerBody<'a> {
    fn from(value: &'a CapabilityIssuer) -> Self {
        Self {
            issuer_id: &value.issuer_id,
            issuer_kind: &value.issuer_kind,
            governance_epoch: value.governance_epoch,
            finalized_receipt_id: &value.finalized_receipt_id,
            key_id: &value.key_id,
            issuer_key_epoch: value.issuer_key_epoch,
            authority_rotation_receipt_id: value.authority_rotation_receipt_id.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityCatalogEntry {
    pub module_id: String,
    pub module_version: String,
    pub namespace: String,
    pub command: String,
    pub schema_version: u32,
    pub schema_hash: String,
    pub max_payload_bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eligible_grant_ids: Vec<String>,
}

impl CapabilityCatalogEntry {
    pub fn validate(&self) -> Result<(), CapabilityAuthorizationValidationError> {
        self.validate_with_work_v1(&mut |_, _| Ok(()))
    }
    pub fn validate_with_fallible_work_v1<E>(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Result<(), CapabilityAuthorizationValidationError>, E> {
        let mut first = None;
        let result = self.validate_with_work_v1(&mut |s, b| match work(s, b) {
            Ok(()) => Ok(()),
            Err(e) => {
                if first.is_none() {
                    first = Some(e)
                }
                Err(CapabilityAuthorizationValidationError::EmptyField(
                    "bounded_work",
                ))
            }
        });
        match first {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }
    pub fn validate_with_work_v1(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
    ) -> Result<(), CapabilityAuthorizationValidationError> {
        work(1, 0)?;
        for (field, value) in [
            ("catalog.entry.module_id", self.module_id.as_str()),
            ("catalog.entry.module_version", self.module_version.as_str()),
            ("catalog.entry.namespace", self.namespace.as_str()),
            ("catalog.entry.command", self.command.as_str()),
            ("catalog.entry.schema_hash", self.schema_hash.as_str()),
        ] {
            required_with_work_v1(field, value, work)?;
        }
        if self.schema_version == 0 || self.max_payload_bytes == 0 {
            return Err(CapabilityAuthorizationValidationError::InvalidBound {
                field: "catalog.entry.schema_version/max_payload_bytes",
                value: 0,
            });
        }
        for grant_id in &self.eligible_grant_ids {
            required_with_work_v1("catalog.entry.eligible_grant_ids", grant_id, work)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityCatalogSnapshot {
    pub snapshot_id: String,
    pub world_id: String,
    pub world_head: u64,
    pub branch_id: String,
    pub finality_epoch: u64,
    pub logical_tick: u64,
    pub module_registry_hash: String,
    pub policy_hash: String,
    pub revocation_epoch: u64,
    pub subject: CapabilitySubject,
    pub presenter: CapabilityPresenter,
    pub audience: CapabilityAudience,
    #[serde(default)]
    pub entries: Vec<CapabilityCatalogEntry>,
    pub valid_until_tick: u64,
}

impl CapabilityCatalogSnapshot {
    pub fn validate(&self) -> Result<(), CapabilityAuthorizationValidationError> {
        self.validate_with_work_v1(&mut |_, _| Ok(()))
    }
    pub fn validate_with_fallible_work_v1<E>(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Result<(), CapabilityAuthorizationValidationError>, E> {
        let mut first = None;
        let result = self.validate_with_work_v1(&mut |s, b| match work(s, b) {
            Ok(()) => Ok(()),
            Err(e) => {
                if first.is_none() {
                    first = Some(e)
                }
                Err(CapabilityAuthorizationValidationError::EmptyField(
                    "bounded_work",
                ))
            }
        });
        match first {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }
    pub fn validate_with_work_v1(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
    ) -> Result<(), CapabilityAuthorizationValidationError> {
        work(1, 0)?;
        required_with_work_v1("catalog.snapshot_id", &self.snapshot_id, work)?;
        required_with_work_v1("catalog.world_id", &self.world_id, work)?;
        required_with_work_v1("catalog.branch_id", &self.branch_id, work)?;
        required_with_work_v1(
            "catalog.module_registry_hash",
            &self.module_registry_hash,
            work,
        )?;
        required_with_work_v1("catalog.policy_hash", &self.policy_hash, work)?;
        self.subject.validate_with_work_v1(work)?;
        self.presenter.validate_with_work_v1(work)?;
        self.audience.validate_with_work_v1(work)?;
        if self.audience.world_id != self.world_id
            || self.audience.branch_id != self.branch_id
            || self.audience.finality_epoch != self.finality_epoch
        {
            return Err(CapabilityAuthorizationValidationError::InvalidBinding(
                "catalog.audience",
            ));
        }
        if self.valid_until_tick < self.logical_tick {
            return Err(CapabilityAuthorizationValidationError::InvalidLifetime {
                issued_at_tick: self.logical_tick,
                expires_at_tick: self.valid_until_tick,
            });
        }
        for entry in &self.entries {
            entry.validate_with_work_v1(work)?;
        }
        Ok(())
    }

    pub fn canonical_hash_input_v1(&self) -> impl Serialize + '_ {
        CapabilityCatalogSnapshotBody {
            world_id: &self.world_id,
            world_head: self.world_head,
            branch_id: &self.branch_id,
            finality_epoch: self.finality_epoch,
            logical_tick: self.logical_tick,
            module_registry_hash: &self.module_registry_hash,
            policy_hash: &self.policy_hash,
            revocation_epoch: self.revocation_epoch,
            subject: &self.subject,
            presenter: &self.presenter,
            audience: &self.audience,
            entries: &self.entries,
            valid_until_tick: self.valid_until_tick,
        }
    }

    pub fn canonical_hash(&self) -> Result<String, CapabilityAuthorizationValidationError> {
        canonical_hash(&self.canonical_hash_input_v1())
    }

    pub fn find_entry(&self, selected: &CapabilityCatalogEntry) -> bool {
        self.entries.iter().any(|entry| {
            entry.module_id == selected.module_id
                && entry.module_version == selected.module_version
                && entry.namespace == selected.namespace
                && entry.command == selected.command
                && entry.schema_version == selected.schema_version
                && entry.schema_hash == selected.schema_hash
                && entry.max_payload_bytes == selected.max_payload_bytes
        })
    }
}

#[derive(Debug, Serialize)]
struct CapabilityCatalogSnapshotBody<'a> {
    world_id: &'a str,
    world_head: u64,
    branch_id: &'a str,
    finality_epoch: u64,
    logical_tick: u64,
    module_registry_hash: &'a str,
    policy_hash: &'a str,
    revocation_epoch: u64,
    subject: &'a CapabilitySubject,
    presenter: &'a CapabilityPresenter,
    audience: &'a CapabilityAudience,
    entries: &'a [CapabilityCatalogEntry],
    valid_until_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCommandResponse {
    pub response_nonce: String,
    pub subject: CapabilitySubject,
    pub presenter: CapabilityPresenter,
    pub audience: CapabilityAudience,
    pub catalog_snapshot_id: String,
    pub selected_entry: CapabilityCatalogEntry,
    pub envelope: ModuleCommandEnvelope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

impl AgentCommandResponse {
    pub fn validate(&self) -> Result<(), CapabilityAuthorizationValidationError> {
        self.validate_with_work_v1(&mut |_, _| Ok(()))
    }
    pub fn validate_with_fallible_work_v1<E>(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Result<(), CapabilityAuthorizationValidationError>, E> {
        let mut first = None;
        let result = self.validate_with_work_v1(&mut |s, b| match work(s, b) {
            Ok(()) => Ok(()),
            Err(e) => {
                if first.is_none() {
                    first = Some(e)
                }
                Err(CapabilityAuthorizationValidationError::EmptyField(
                    "bounded_work",
                ))
            }
        });
        match first {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }
    pub fn validate_with_work_v1(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
    ) -> Result<(), CapabilityAuthorizationValidationError> {
        work(1, 0)?;
        required_with_work_v1("response.response_nonce", &self.response_nonce, work)?;
        self.subject.validate_with_work_v1(work)?;
        self.presenter.validate_with_work_v1(work)?;
        self.audience.validate_with_work_v1(work)?;
        required_with_work_v1(
            "response.catalog_snapshot_id",
            &self.catalog_snapshot_id,
            work,
        )?;
        self.selected_entry.validate_with_work_v1(work)?;
        if !self.selected_entry.eligible_grant_ids.is_empty() {
            return Err(
                CapabilityAuthorizationValidationError::ForbiddenAuthorityField(
                    "response.selected_entry.eligible_grant_ids",
                ),
            );
        }
        optional_required_with_work_v1("response.provider_id", self.provider_id.as_deref(), work)?;
        optional_required_with_work_v1("response.trace_id", self.trace_id.as_deref(), work)?;
        if self
            .provider_id
            .as_deref()
            .is_some_and(|id| id != self.presenter.presenter_id)
        {
            return Err(CapabilityAuthorizationValidationError::InvalidBinding(
                "response.provider_id/presenter.presenter_id",
            ));
        }
        if self.envelope.namespace != self.selected_entry.namespace
            || self.envelope.name != self.selected_entry.command
            || self.envelope.schema_version != self.selected_entry.schema_version
            || self.envelope.schema_hash != self.selected_entry.schema_hash
            || self.envelope.payload.len() > self.selected_entry.max_payload_bytes as usize
        {
            return Err(CapabilityAuthorizationValidationError::InvalidBinding(
                "response.envelope/selected_entry",
            ));
        }
        Ok(())
    }

    pub fn canonical_request_hash(&self) -> Result<String, CapabilityAuthorizationValidationError> {
        canonical_hash(self)
    }

    pub fn matches_catalog(&self, snapshot: &CapabilityCatalogSnapshot) -> bool {
        self.validate().is_ok()
            && snapshot.validate().is_ok()
            && self.catalog_snapshot_id == snapshot.snapshot_id
            && self.subject == snapshot.subject
            && self.presenter == snapshot.presenter
            && self.audience == snapshot.audience
            && snapshot.find_entry(&self.selected_entry)
    }
}

pub type CatalogSnapshot = CapabilityCatalogSnapshot;
pub type CatalogEntry = CapabilityCatalogEntry;
pub type CapabilitySelectedCatalogEntry = CapabilityCatalogEntry;
pub type CapabilityCatalogSelectedEntry = CapabilityCatalogEntry;
pub type SelectedCatalogEntry = CapabilityCatalogEntry;
pub type CatalogSnapshotEntry = CapabilityCatalogEntry;
pub type CapabilityAgentCommandResponse = AgentCommandResponse;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityAuthorizationValidationError {
    EmptyField(&'static str),
    InvalidEnum {
        field: &'static str,
        value: String,
    },
    InvalidSelector {
        field: &'static str,
        value: String,
    },
    InvalidBound {
        field: &'static str,
        value: u64,
    },
    InvalidLifetime {
        issued_at_tick: u64,
        expires_at_tick: u64,
    },
    InvalidBinding(&'static str),
    ForbiddenAuthorityField(&'static str),
    UnsupportedVersion(u32),
    CanonicalEncoding(String),
}

impl fmt::Display for CapabilityAuthorizationValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(formatter, "{field} must not be empty"),
            Self::InvalidEnum { field, value } => write!(formatter, "invalid {field}: {value}"),
            Self::InvalidSelector { field, value } => {
                write!(formatter, "invalid {field} selector: {value}")
            }
            Self::InvalidBound { field, value } => {
                write!(formatter, "invalid {field} bound: {value}")
            }
            Self::InvalidLifetime {
                issued_at_tick,
                expires_at_tick,
            } => write!(
                formatter,
                "expiry {expires_at_tick} precedes issue tick {issued_at_tick}"
            ),
            Self::InvalidBinding(field) => write!(formatter, "invalid capability binding: {field}"),
            Self::ForbiddenAuthorityField(field) => {
                write!(formatter, "forbidden authority field: {field}")
            }
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported capability version: {version}")
            }
            Self::CanonicalEncoding(error) => {
                write!(formatter, "canonical capability encoding failed: {error}")
            }
        }
    }
}

impl std::error::Error for CapabilityAuthorizationValidationError {}

fn required(
    field: &'static str,
    value: &str,
) -> Result<(), CapabilityAuthorizationValidationError> {
    required_with_work_v1(field, value, &mut |_, _| Ok(()))
}
fn required_with_work_v1(
    field: &'static str,
    value: &str,
    work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
) -> Result<(), CapabilityAuthorizationValidationError> {
    work(1, value.len() as u64)?;
    if value.trim().is_empty() {
        Err(CapabilityAuthorizationValidationError::EmptyField(field))
    } else {
        Ok(())
    }
}

fn optional_required(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), CapabilityAuthorizationValidationError> {
    optional_required_with_work_v1(field, value, &mut |_, _| Ok(()))
}
fn optional_required_with_work_v1(
    field: &'static str,
    value: Option<&str>,
    work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
) -> Result<(), CapabilityAuthorizationValidationError> {
    value.map_or(Ok(()), |value| required_with_work_v1(field, value, work))
}

fn validate_selectors(
    field: &'static str,
    selectors: Option<&[String]>,
) -> Result<(), CapabilityAuthorizationValidationError> {
    validate_selectors_with_work_v1(field, selectors, &mut |_, _| Ok(()))
}
fn validate_selectors_with_work_v1(
    field: &'static str,
    selectors: Option<&[String]>,
    work: &mut dyn FnMut(u64, u64) -> Result<(), CapabilityAuthorizationValidationError>,
) -> Result<(), CapabilityAuthorizationValidationError> {
    let Some(selectors) = selectors else {
        return Ok(());
    };
    if selectors.is_empty() {
        return Err(CapabilityAuthorizationValidationError::InvalidSelector {
            field,
            value: "empty selector set".to_string(),
        });
    }
    for selector in selectors {
        required_with_work_v1(field, selector, work)?;
        if selector == "*" {
            return Err(CapabilityAuthorizationValidationError::InvalidSelector {
                field,
                value: selector.clone(),
            });
        }
    }
    Ok(())
}

fn bound_contains(parent: Option<u64>, child: Option<u64>) -> bool {
    matches!((parent, child), (Some(parent), Some(child)) if child <= parent)
}

fn option_contains(parent: Option<&str>, child: Option<&str>) -> bool {
    matches!((parent, child), (Some(parent), Some(child)) if parent == child)
}

fn operation_contains(parent: &str, child: &str) -> bool {
    parent == child || (parent == "write" && child == "read")
}

pub fn canonical_hash<T: Serialize>(
    value: &T,
) -> Result<String, CapabilityAuthorizationValidationError> {
    let bytes = encode_canonical_cbor(value).map_err(|error| {
        CapabilityAuthorizationValidationError::CanonicalEncoding(error.to_string())
    })?;
    canonical_sha256_hex(&bytes)
}

pub fn canonical_sha256_hex(
    bytes: &[u8],
) -> Result<String, CapabilityAuthorizationValidationError> {
    Ok(hex_lower(&sha256_digest(bytes)))
}

pub fn capability_scope_hash(
    scope: &CapabilityScope,
) -> Result<String, CapabilityAuthorizationValidationError> {
    canonical_hash(scope)
}

pub fn canonical_scope_hash(
    scope: &CapabilityScope,
) -> Result<String, CapabilityAuthorizationValidationError> {
    capability_scope_hash(scope)
}

pub fn capability_grant_body_hash(
    grant: &CapabilityGrantV2,
) -> Result<String, CapabilityAuthorizationValidationError> {
    grant.canonical_body_hash()
}

pub fn canonical_grant_body_hash(
    grant: &CapabilityGrantV2,
) -> Result<String, CapabilityAuthorizationValidationError> {
    capability_grant_body_hash(grant)
}

pub fn capability_request_hash(
    response: &AgentCommandResponse,
) -> Result<String, CapabilityAuthorizationValidationError> {
    response.canonical_request_hash()
}

pub fn canonical_request_hash(
    response: &AgentCommandResponse,
) -> Result<String, CapabilityAuthorizationValidationError> {
    response.canonical_request_hash()
}

pub fn scope_matches_exact(grant: &CapabilityScope, requested: &CapabilityScope) -> bool {
    grant.matches_exact(requested)
}

pub fn scope_allows(grant: &CapabilityScope, requested: &CapabilityScope) -> bool {
    grant.allows(requested)
}

pub fn scope_is_subset(child: &CapabilityScope, parent: &CapabilityScope) -> bool {
    child.is_subset_of(parent)
}

pub fn validate_capability_grant_v2(
    grant: &CapabilityGrantV2,
) -> Result<(), CapabilityAuthorizationValidationError> {
    grant.validate()
}

pub fn validate_capability_catalog_snapshot(
    snapshot: &CapabilityCatalogSnapshot,
) -> Result<(), CapabilityAuthorizationValidationError> {
    snapshot.validate()
}

pub fn validate_agent_command_response(
    response: &AgentCommandResponse,
) -> Result<(), CapabilityAuthorizationValidationError> {
    response.validate()
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[path = "capability_sha256_stream.rs"]
mod sha256_stream;
fn sha256_digest(input: &[u8]) -> [u8; 32] {
    let mut state = sha256_stream::Sha256V1::new();
    state
        .update(input, &mut |_, _| {
            Ok::<_, crate::CanonicalCborStreamError>(())
        })
        .expect("NOOP SHA work");
    state
        .finish(&mut |_, _| Ok::<_, crate::CanonicalCborStreamError>(()))
        .expect("NOOP SHA work")
}
#[path = "capability_canonical_work.rs"]
mod canonical_work;
pub use canonical_work::*;

fn selectors_contain_with_work_v1<E>(
    parent: Option<&[String]>,
    child: Option<&[String]>,
    work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
) -> Result<bool, E> {
    match (parent, child) {
        (Some(parent), Some(child)) => {
            for item in child {
                let mut found = false;
                for candidate in parent {
                    work(1, (candidate.len() + item.len()) as u64)?;
                    if candidate == item {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

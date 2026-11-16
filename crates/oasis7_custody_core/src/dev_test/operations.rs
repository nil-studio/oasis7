//! Platform-independent wire originals and bounded ledger transitions.
//! Deserialization is not authorization or proof of state freshness.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Arguments {
    Context,
    Move {
        context_request_id: String,
        to: [i64; 3],
    },
    Lookup {
        original_request_id: String,
    },
}
// Serde's internally tagged unit variants accept additional fields even with
// deny_unknown_fields. An empty struct variant keeps Context a closed DTO while
// preserving the public enum API and its existing serialized representation.
impl<'de> Deserialize<'de> for Arguments {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Context {},
            Move {
                context_request_id: String,
                to: [i64; 3],
            },
            Lookup {
                original_request_id: String,
            },
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Context {} => Self::Context,
            Wire::Move {
                context_request_id,
                to,
            } => Self::Move {
                context_request_id,
                to,
            },
            Wire::Lookup {
                original_request_id,
            } => Self::Lookup {
                original_request_id,
            },
        })
    }
}

impl Arguments {
    pub fn method(&self) -> &'static str {
        match self {
            Self::Context => "context",
            Self::Move { .. } => "move",
            Self::Lookup { .. } => "lookup",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub request_id: String,
    pub arguments: Arguments,
}
impl Call {
    /// Recovery follows the Context/Move being observed, never a Lookup of a Lookup.
    pub fn recovery_request_id(&self) -> &str {
        match &self.arguments {
            Arguments::Lookup {
                original_request_id,
            } => original_request_id,
            Arguments::Context | Arguments::Move { .. } => &self.request_id,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub account_ref: String,
    pub environment: String,
    pub world: String,
    pub genesis_digest: String,
    pub agent: String,
    pub resource: String,
    pub executor: String,
    pub fence_digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Original {
    pub body_digest: String,
    pub scope: Scope,
    pub arguments: Arguments,
    pub native_context: String,
    pub native_kind: String,
    #[serde(default = "lookup_only_by_default")]
    pub historical_lookup_only: bool,
    #[serde(default)]
    pub native_action: Option<[i64; 3]>,
    #[serde(default)]
    pub native_context_identity: Option<oasis7_agent_api::game::TaskContextIdentityV1>,
    pub last_observation: Option<serde_json::Value>,
}
fn lookup_only_by_default() -> bool {
    true
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    pub version: u32,
    pub used: u64,
    pub operations: BTreeMap<String, Original>,
}

pub fn usage_key(grant: &str, caller: &str) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(&(grant.len() as u64).to_be_bytes());
    hash.update(grant.as_bytes());
    hash.update(&(caller.len() as u64).to_be_bytes());
    hash.update(caller.as_bytes());
    hash.finalize().to_hex().to_string()
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

impl Ledger {
    /// Call only inside the same durable transaction as admission and budget.
    pub fn admit(
        &mut self,
        grant_id: &str,
        caller: &str,
        scope: &Scope,
        call: &Call,
        max_calls: u64,
        mut history: impl FnMut(&str) -> Result<Option<Original>, &'static str>,
    ) -> Result<(Original, bool), &'static str> {
        if !valid_id(&call.request_id) {
            return Err("invalid_request_id");
        }
        let digest = blake3::hash(
            &serde_json::to_vec(&call.arguments).map_err(|_| "invalid_operation_body")?,
        )
        .to_hex()
        .to_string();
        let ledger = self;
        if let Some(original) = ledger.operations.get(&call.request_id) {
            if original.body_digest != digest || original.scope != *scope {
                return Err("request_body_conflict");
            }
            if ledger.used >= max_calls {
                return Err("call_limit_exhausted");
            }
            let original = original.clone();
            ledger.used += 1;
            return Ok((original, false));
        }
        if ledger.used >= max_calls {
            return Err("call_limit_exhausted");
        }
        if ledger.operations.len() >= 4096 {
            return Err("operation_capacity_exhausted");
        }
        let (
            native_context,
            native_kind,
            native_action,
            native_context_identity,
            historical_lookup_only,
        ) = match &call.arguments {
            Arguments::Context => {
                let domain = serde_json::to_vec(&(grant_id, caller, scope, &call.request_id))
                    .map_err(|_| "invalid_operation_body")?;
                (
                    format!("dev-test-{}", blake3::hash(&domain).to_hex()),
                    "prefix".into(),
                    None,
                    None,
                    false,
                )
            }
            Arguments::Move {
                context_request_id, ..
            } => {
                let context = ledger
                    .operations
                    .get(context_request_id)
                    .ok_or("original_context_unknown")?;
                if context.scope != *scope || !matches!(context.arguments, Arguments::Context) {
                    return Err("original_scope_denied");
                }
                if ledger.operations.values().any(|op| {
                    op.native_context == context.native_context
                        && matches!(op.arguments, Arguments::Move { .. })
                }) {
                    return Err("context_action_already_reserved");
                }
                let Arguments::Move { to, .. } = &call.arguments else {
                    unreachable!()
                };
                let identity = context
                    .native_context_identity
                    .clone()
                    .ok_or("original_context_unverified")?;
                (
                    context.native_context.clone(),
                    "action".into(),
                    Some(*to),
                    Some(identity),
                    false,
                )
            }
            Arguments::Lookup {
                original_request_id,
            } => {
                let (original, historical) = match ledger.operations.get(original_request_id) {
                    Some(original) => (original.clone(), false),
                    None => (
                        history(original_request_id)?.ok_or("original_operation_unknown")?,
                        true,
                    ),
                };
                if original.scope != *scope
                    || matches!(original.arguments, Arguments::Lookup { .. })
                {
                    return Err("original_scope_denied");
                }
                let native_action = match &original.arguments {
                    Arguments::Move { to, .. } => Some(*to),
                    _ => original.native_action,
                };
                (
                    original.native_context.clone(),
                    original.native_kind.clone(),
                    native_action,
                    original.native_context_identity.clone(),
                    historical,
                )
            }
        };
        let original = Original {
            body_digest: digest,
            scope: scope.clone(),
            arguments: call.arguments.clone(),
            native_context,
            native_kind,
            historical_lookup_only,
            native_action,
            native_context_identity,
            last_observation: None,
        };
        ledger.used += 1;
        ledger
            .operations
            .insert(call.request_id.clone(), original.clone());
        Ok((original, true))
    }
    /// Retains provenance without persisting private Goal text. Failure leaves self unchanged.
    pub fn observe(
        &mut self,
        call: &Call,
        observation: serde_json::Value,
    ) -> Result<(), &'static str> {
        let identity = observation
            .get("context")
            .map(|value| {
                serde_json::from_value::<oasis7_agent_api::game::TaskContextIdentityV1>(
                    value.clone(),
                )
                .map_err(|_| "invalid_context_observation")
            })
            .transpose()?;
        // Private goal content is delivered only in the authorized tool reply.
        // Recovery rereads the SDK proof; the usage journal does not log the text.
        let mut retained = observation.clone();
        if let Some(object) = retained.as_object_mut() {
            object.remove("active_goal");
        }
        let mut next = self.clone();
        let ledger = &mut next;
        let original = ledger
            .operations
            .get(&call.request_id)
            .ok_or("original_operation_unknown")?;
        let mut update = vec![call.request_id.clone()];
        if identity.is_some()
            && let Arguments::Lookup {
                original_request_id,
            } = &original.arguments
            && ledger.operations.contains_key(original_request_id)
        {
            update.push(original_request_id.clone());
        }
        for reference in update {
            let original = ledger
                .operations
                .get_mut(&reference)
                .ok_or("original_operation_unknown")?;
            if let Some(identity) = &identity {
                if original
                    .native_context_identity
                    .as_ref()
                    .is_some_and(|prior| prior != identity)
                {
                    return Err("native_original_body_conflict");
                }
                original.native_context_identity = Some(identity.clone());
            }
            original.last_observation = Some(retained.clone());
        }
        *self = next;
        Ok(())
    }
}

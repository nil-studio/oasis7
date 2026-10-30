//! Borrowed current economy fields and original journals; no hydration/authority.
use super::*;
use crate::world::cognition_economy::{COGNITION_ECONOMY_SCHEMA_VERSION, CognitionEconomyStateV1};

macro_rules! field {
    ($sink:expr, $object:expr, $name:ident, text, $namespace:ident) => {{
        let value = &$object[stringify!($name)];
        if value.as_str().is_none() {
            return Err(ProjectionError::Invalid("economy text field shape"));
        }
        state_value(
            $sink,
            concat!("economy.", stringify!($name)),
            Namespace::$namespace,
            value,
        )
    }};
    ($sink:expr, $object:expr, $name:ident, number, $namespace:ident) => {{
        let value = &$object[stringify!($name)];
        if value.as_u64().is_none() {
            return Err(ProjectionError::Invalid("economy cursor shape"));
        }
        state_value(
            $sink,
            concat!("economy.", stringify!($name)),
            Namespace::$namespace,
            value,
        )
    }};
    ($sink:expr, $object:expr, $name:ident, map, $namespace:ident) => {
        state_map(
            $sink,
            concat!("economy.", stringify!($name)),
            Namespace::$namespace,
            &$object[stringify!($name)],
        )
    };
    ($sink:expr, $object:expr, $name:ident, nested, $namespace:ident) => {
        nested_balances(
            $sink,
            concat!("economy.", stringify!($name)),
            Namespace::$namespace,
            &$object[stringify!($name)],
        )
    };
    ($sink:expr, $object:expr, $name:ident, history, $namespace:ident) => {{
        let values = $object[stringify!($name)]
            .as_array()
            .ok_or(ProjectionError::Invalid("economy history shape"))?;
        $sink
            .side(
                descriptor(
                    concat!("economy.", stringify!($name)),
                    FieldDestination::History,
                    FieldShape::History,
                ),
                values,
                Some(values.len() as u64),
            )
            .map_err(ProjectionError::Sink)
    }};
}
macro_rules! schema {
    ($($name:ident => $mode:ident, $namespace:ident;)*) => {
        const KEYS: &[&str] = &[$(stringify!($name),)*];
        // This checks field coverage without deserializing/cloning the ledger.
        const _: fn(&CognitionEconomyStateV1) = |state| {
            let CognitionEconomyStateV1 { $($name: _,)* } = state;
        };
        pub(super) fn visit<S: GenesisFieldVisitor>(value: &Value, sink: &mut S) -> Result<(), ProjectionError<S::Error>> {
            let object = value.as_object().ok_or(ProjectionError::Invalid("economy object"))?;
            if object.len() != KEYS.len() || object.keys().any(|key| !KEYS.contains(&key.as_str())) {
                return Err(ProjectionError::Invalid("unclassified or missing economy field"));
            }
            if object["schema_version"].as_str() != Some(COGNITION_ECONOMY_SCHEMA_VERSION) {
                return Err(ProjectionError::Invalid("economy schema"));
            }
            $(field!(sink, object, $name, $mode, $namespace)?;)*
            Ok(())
        }
    }
}
schema! {
    schema_version => text, Artifacts;
    balances => nested, Business;
    provisioned_balances => nested, Business;
    lease_binding_keys => map, Authorization;
    provisions => map, Authorization;
    provision_receipts => map, Authorization;
    provision_journal => history, History;
    provision_head_seq => number, Allocator;
    provision_head_digest => text, Allocator;
    leases => map, Effects;
    receipts => map, Authorization;
    idempotency => map, Authorization;
    operations => map, Authorization;
    journal => history, History;
    head_seq => number, Allocator;
    head_digest => text, Allocator;
}

fn nested_balances<S: GenesisFieldVisitor>(
    sink: &mut S,
    name: &'static str,
    namespace: Namespace,
    value: &Value,
) -> Result<(), ProjectionError<S::Error>> {
    let accounts = value
        .as_object()
        .ok_or(ProjectionError::Invalid("economy balance map shape"))?;
    let field = descriptor(
        name,
        FieldDestination::State(namespace),
        FieldShape::NestedMap,
    );
    sink.begin(field, None).map_err(ProjectionError::Sink)?;
    for (account, resources) in accounts {
        let resources = resources
            .as_object()
            .ok_or(ProjectionError::Invalid("economy resource map shape"))?;
        sink.group(field, account, resources.len())
            .map_err(ProjectionError::Sink)?;
        for (resource, balance) in resources {
            sink.entry(field, &(account, resource), balance)
                .map_err(ProjectionError::Sink)?;
        }
    }
    sink.end(field).map_err(ProjectionError::Sink)
}

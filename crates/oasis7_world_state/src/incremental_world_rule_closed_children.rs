//! Closed original global child keys, never a payload/family qualification.
use crate::incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
pub const WORLD_RULE_CHILDREN_V1: [&str; 13] = [
    "requests",
    "governance_effects",
    "organization_cases",
    "organization_estate",
    "supply_authorizations",
    "supply_batches",
    "supply_lineages",
    "supply_shipments",
    "organization_continuity",
    "emergency_supply",
    "agent_orders",
    "industrial_work",
    "substitution",
];
/// Complete global semantic-key shape only. Nested RecordRootRef trees remain
/// the selected family's responsibility; Unit identifies the optional parent.
pub fn world_rule_child_from_semantic_v1(
    bytes: &[u8],
) -> Result<Option<&'static str>, ValueTreeError> {
    let bad = || ValueTreeError::Invalid("closed WorldRules13 semantic child");
    let decoded = read_semantic_key_v1(bytes).map_err(ValueTreeError::Invalid)?;
    if matches!(decoded, SemanticKeyRefV1::Unit) {
        return Ok(None);
    }
    let SemanticKeyRefV1::Pair { left, right } = decoded else {
        return Err(bad());
    };
    let (name, id, column) = match left.decode().map_err(ValueTreeError::Invalid)? {
        SemanticKeyRefV1::Text(name) => {
            (name, right.decode().map_err(ValueTreeError::Invalid)?, None)
        }
        SemanticKeyRefV1::Pair { left, right: id } => {
            let SemanticKeyRefV1::Text(name) = left.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(bad());
            };
            let SemanticKeyRefV1::Text(column) = right.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(bad());
            };
            (
                name,
                id.decode().map_err(ValueTreeError::Invalid)?,
                Some(column),
            )
        }
        _ => return Err(bad()),
    };
    let valid = match (name, id, column) {
        (
            "requests"
            | "organization_continuity"
            | "emergency_supply"
            | "agent_orders"
            | "industrial_work"
            | "substitution",
            SemanticKeyRefV1::Unit,
            None,
        ) => true,
        (
            "governance_effects"
            | "organization_cases"
            | "organization_estate"
            | "supply_authorizations"
            | "supply_batches",
            SemanticKeyRefV1::Text(_),
            None,
        ) => true,
        (
            "supply_lineages",
            SemanticKeyRefV1::Text(_),
            None | Some("target" | "fulfilled" | "reserved"),
        ) => true,
        (
            "supply_shipments",
            SemanticKeyRefV1::Unsigned(_),
            None
            | Some(
                "authorization_id" | "batch_id" | "beneficiary_id" | "lineage_id" | "input_amount"
                | "delivered_amount" | "settled",
            ),
        ) => true,
        _ => false,
    };
    if !valid {
        return Err(bad());
    }
    WORLD_RULE_CHILDREN_V1
        .iter()
        .copied()
        .find(|known| *known == name)
        .map(Some)
        .ok_or_else(bad)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::incremental_semantic_key::write_semantic_key_v1;
    #[test]
    fn original13_names_parent_and_flat_columns_are_closed() {
        for name in WORLD_RULE_CHILDREN_V1 {
            let mut bytes = Vec::new();
            match name {
                "requests"
                | "organization_continuity"
                | "emergency_supply"
                | "agent_orders"
                | "industrial_work"
                | "substitution" => {
                    write_semantic_key_v1(&(name, ()), &mut bytes).unwrap();
                }
                "supply_shipments" => {
                    write_semantic_key_v1(&(name, u64::MAX), &mut bytes).unwrap();
                }
                _ => {
                    write_semantic_key_v1(&(name, " original key "), &mut bytes).unwrap();
                }
            }
            assert_eq!(
                world_rule_child_from_semantic_v1(&bytes).unwrap(),
                Some(name)
            );
        }
        let mut bytes = Vec::new();
        write_semantic_key_v1(&(("supply_shipments", u64::MAX), "settled"), &mut bytes).unwrap();
        assert_eq!(
            world_rule_child_from_semantic_v1(&bytes).unwrap(),
            Some("supply_shipments")
        );
        bytes.clear();
        write_semantic_key_v1(&(), &mut bytes).unwrap();
        assert_eq!(world_rule_child_from_semantic_v1(&bytes).unwrap(), None);
        for name in ["unknown-child", "requests", "supply_shipments"] {
            bytes.clear();
            write_semantic_key_v1(&(name, "bad-key-type"), &mut bytes).unwrap();
            assert!(world_rule_child_from_semantic_v1(&bytes).is_err());
        }
        bytes.clear();
        write_semantic_key_v1(&(("supply_lineages", "id"), "unknown_column"), &mut bytes).unwrap();
        assert!(world_rule_child_from_semantic_v1(&bytes).is_err());
    }
}

//! Finite transport/render budgets. A profile conveys no authorization.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum DisplayProfile {
    #[default]
    #[serde(rename = "legacy_snapshot_v1")]
    Legacy,
    #[serde(rename = "world_view_v1")]
    WorldView,
}

impl DisplayProfile {
    pub(crate) fn bytes(self) -> usize {
        match self {
            Self::Legacy => 256 * 1024,
            Self::WorldView => 2 * 1024 * 1024,
        }
    }
    pub(crate) fn markers(self) -> usize {
        match self {
            Self::Legacy => 500,
            Self::WorldView => 1000,
        }
    }
    pub(crate) fn labels(self) -> usize {
        match self {
            Self::Legacy => 100,
            Self::WorldView => 200,
        }
    }
    pub(crate) fn validate_structure(self, value: &Value) -> Result<(), String> {
        self.structure_cost_at(value, 0).map(|_| ())
    }
    pub(crate) fn nodes(self) -> usize {
        match self {
            Self::Legacy => 16384,
            Self::WorldView => 131072,
        }
    }
    pub(crate) fn structure_cost_at(self, value: &Value, depth: usize) -> Result<usize, String> {
        let mut remaining = self.nodes();
        fn visit(value: &Value, depth: usize, remaining: &mut usize, items: usize) -> bool {
            if depth > 16 || *remaining == 0 {
                return false;
            }
            *remaining -= 1;
            match value {
                Value::String(text) => text.encode_utf16().count() <= 8192,
                Value::Array(values) => {
                    values.len() <= items
                        && values.iter().all(|v| visit(v, depth + 1, remaining, items))
                }
                Value::Object(fields) => {
                    fields.len() <= 500
                        && fields
                            .values()
                            .all(|v| visit(v, depth + 1, remaining, items))
                }
                _ => true,
            }
        }
        if visit(value, depth, &mut remaining, self.markers()) {
            Ok(self.nodes() - remaining)
        } else {
            Err("pixel world display structure capacity exceeded".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn profiles_are_finite_and_do_not_accept_arbitrary_caps() {
        assert_eq!(DisplayProfile::default().markers(), 500);
        assert_eq!(DisplayProfile::WorldView.labels(), 200);
        assert!(serde_json::from_value::<DisplayProfile>(json!("world_view_v2")).is_err());
        assert!(serde_json::from_value::<DisplayProfile>(json!(1000)).is_err());
        assert!(serde_json::from_value::<DisplayProfile>(Value::Null).is_err());
        let array = json!(vec![0; 1000]);
        assert!(DisplayProfile::WorldView.validate_structure(&array).is_ok());
        assert!(DisplayProfile::Legacy.validate_structure(&array).is_err());
        assert!(
            DisplayProfile::WorldView
                .validate_structure(&json!(vec![0; 1001]))
                .is_err()
        );
        assert!(
            DisplayProfile::WorldView
                .validate_structure(&json!("😀".repeat(4097)))
                .is_err()
        );
    }
}

//! Closed V2 selection: Move0 is not a ResourceBasis or ControlOnly grant.
use super::*;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExactMoveSelectionV2 {
    Move { position_cm: [DecimalI64; 3] },
}
/// Resource-only JSON/CBOR remains the existing raw resource selection shape.
/// Each alternative rejects unknown fields; there is no arbitrary JSON fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApprovedActionSelectionV2 {
    ExactMove0(ExactMoveSelectionV2),
    Resource(wire::ResourceBasisSelectionV1),
}
impl From<wire::ResourceBasisSelectionV1> for ApprovedActionSelectionV2 {
    fn from(v: wire::ResourceBasisSelectionV1) -> Self {
        Self::Resource(v)
    }
}
impl ApprovedActionSelectionV2 {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::ExactMove0(_) => Ok(()),
            Self::Resource(v) => v.validate(),
        }
    }
    pub fn is_exact_move0(&self) -> bool {
        matches!(self, Self::ExactMove0(_))
    }
    pub fn resource(&self) -> Option<&wire::ResourceBasisSelectionV1> {
        match self {
            Self::Resource(v) => Some(v),
            Self::ExactMove0(_) => None,
        }
    }
    pub fn approved_action(&self) -> ApprovedActionV1 {
        match self {
            Self::ExactMove0(ExactMoveSelectionV2::Move { position_cm }) => {
                ApprovedActionV1::Move {
                    position_cm: *position_cm,
                }
            }
            Self::Resource(v) => resource_action(v),
        }
    }
    pub fn required_resource_refs(&self) -> Vec<String> {
        match self.resource() {
            None => vec![],
            Some(wire::ResourceBasisSelectionV1::Transfer {
                from,
                to,
                material,
                routing,
                ..
            }) => {
                let mut refs = vec![from.reference(), to.reference(), material.clone()];
                if let wire::ResourceRoutingV1::Routes { route_ids } = routing {
                    refs.extend(route_ids.clone());
                }
                refs
            }
            Some(wire::ResourceBasisSelectionV1::Build { site, factory, .. }) => {
                vec![site.clone(), factory.clone()]
            }
            Some(wire::ResourceBasisSelectionV1::Schedule {
                factory, recipe, ..
            }) => vec![factory.clone(), recipe.clone()],
        }
    }
}
fn resource_action(s: &wire::ResourceBasisSelectionV1) -> ApprovedActionV1 {
    match s {
        wire::ResourceBasisSelectionV1::Transfer {
            from,
            to,
            material,
            amount,
            distance_km,
            priority,
            routing,
        } => ApprovedActionV1::Transfer {
            from: from.clone().into(),
            to: to.clone().into(),
            material: material.clone(),
            amount: DecimalI64(amount.0),
            distance_km: DecimalI64(distance_km.0),
            priority: *priority,
            routing: match routing {
                wire::ResourceRoutingV1::Direct => ApprovedRoutingV1::Direct,
                wire::ResourceRoutingV1::Routes { route_ids } => ApprovedRoutingV1::Routes {
                    route_ids: route_ids.clone(),
                },
            },
        },
        wire::ResourceBasisSelectionV1::Build {
            site,
            factory,
            factory_kind,
        } => ApprovedActionV1::Build {
            site: site.clone(),
            factory: factory.clone(),
            factory_kind: factory_kind.clone(),
        },
        wire::ResourceBasisSelectionV1::Schedule {
            factory,
            recipe,
            batches,
        } => ApprovedActionV1::Schedule {
            factory: factory.clone(),
            recipe: recipe.clone(),
            batches: *batches,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_selection_wrapper_preserves_json_and_canonical_cbor() {
        let resource = wire::ResourceBasisSelectionV1::Build {
            site: "site".into(),
            factory: "factory".into(),
            factory_kind: "smelter".into(),
        };
        let selected = ApprovedActionSelectionV2::Resource(resource.clone());
        assert_eq!(
            serde_json::to_value(&selected).unwrap(),
            serde_json::to_value(&resource).unwrap()
        );
        assert_eq!(
            oasis7_wasm_abi::encode_canonical_cbor(&selected).unwrap(),
            oasis7_wasm_abi::encode_canonical_cbor(&resource).unwrap()
        );
        assert_eq!(
            serde_json::from_value::<ApprovedActionSelectionV2>(
                serde_json::to_value(&resource).unwrap()
            )
            .unwrap(),
            selected
        );
        assert!(!selected.is_exact_move0());
    }
}

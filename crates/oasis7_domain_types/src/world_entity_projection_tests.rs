use super::{
    GeoPos, ModuleVisualAnchor, ModuleVisualAnchorProjectionDataV1, ModuleVisualEntity,
    ModuleVisualEntityProjectionFieldsV1,
};
use serde::Serialize;

// Preserve the previous derive definitions as an independent encoding oracle.
#[derive(Serialize)]
#[serde(
    rename = "ModuleVisualAnchor",
    tag = "type",
    content = "data",
    rename_all = "snake_case"
)]
enum OriginalAnchor {
    Agent { agent_id: String },
    Location { location_id: String },
    Absolute { pos: GeoPos },
}

#[derive(Serialize)]
#[serde(rename = "ModuleVisualEntity")]
struct OriginalEntity {
    entity_id: String,
    module_id: String,
    kind: String,
    label: Option<String>,
    anchor: OriginalAnchor,
}

fn original_anchor(anchor: &ModuleVisualAnchor) -> OriginalAnchor {
    match anchor {
        ModuleVisualAnchor::Agent { agent_id } => OriginalAnchor::Agent {
            agent_id: agent_id.clone(),
        },
        ModuleVisualAnchor::Location { location_id } => OriginalAnchor::Location {
            location_id: location_id.clone(),
        },
        ModuleVisualAnchor::Absolute { pos } => OriginalAnchor::Absolute { pos: *pos },
    }
}

fn borrowed_anchor(anchor: &ModuleVisualAnchor) -> ModuleVisualAnchorProjectionDataV1<'_, str> {
    match anchor {
        ModuleVisualAnchor::Agent { agent_id } => ModuleVisualAnchorProjectionDataV1::Agent {
            agent_id: agent_id.as_str(),
        },
        ModuleVisualAnchor::Location { location_id } => {
            ModuleVisualAnchorProjectionDataV1::Location {
                location_id: location_id.as_str(),
            }
        }
        ModuleVisualAnchor::Absolute { pos } => {
            ModuleVisualAnchorProjectionDataV1::Absolute { pos }
        }
    }
}

fn cbor<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes).unwrap();
    bytes
}

fn assert_original_encodings(entity: &ModuleVisualEntity) {
    let original = OriginalEntity {
        entity_id: entity.entity_id.clone(),
        module_id: entity.module_id.clone(),
        kind: entity.kind.clone(),
        label: entity.label.clone(),
        anchor: original_anchor(&entity.anchor),
    };
    let anchor = borrowed_anchor(&entity.anchor);
    let fields = ModuleVisualEntityProjectionFieldsV1 {
        entity_id: entity.entity_id.as_str(),
        module_id: entity.module_id.as_str(),
        kind: entity.kind.as_str(),
        label: entity.label.as_deref(),
        anchor: &anchor,
    };
    let expected_json = serde_json::to_vec(&original).unwrap();
    let expected_cbor = cbor(&original);
    assert_eq!(serde_json::to_vec(entity).unwrap(), expected_json);
    assert_eq!(serde_json::to_vec(&fields).unwrap(), expected_json);
    assert_eq!(cbor(entity), expected_cbor);
    assert_eq!(cbor(&fields), expected_cbor);
    assert_eq!(
        serde_json::to_vec(&anchor).unwrap(),
        serde_json::to_vec(&original.anchor).unwrap()
    );
    assert_eq!(cbor(&anchor), cbor(&original.anchor));
    assert_eq!(
        serde_json::from_slice::<ModuleVisualEntity>(&expected_json).unwrap(),
        *entity
    );
    assert_eq!(
        ciborium::de::from_reader::<ModuleVisualEntity, _>(expected_cbor.as_slice()).unwrap(),
        *entity
    );
}

#[test]
fn owned_and_borrowed_fields_match_original_json_and_raw_cbor() {
    let anchors = [
        ModuleVisualAnchor::Agent {
            agent_id: "agent/内部\0\"".into(),
        },
        ModuleVisualAnchor::Location {
            location_id: "location/🙂\n".into(),
        },
        ModuleVisualAnchor::Absolute {
            pos: GeoPos::new(i64::MIN, -2, i64::MAX),
        },
    ];
    for anchor in anchors {
        for label in [None, Some(""), Some("label/标记\n\"\\\0🙂")] {
            let entity = ModuleVisualEntity {
                entity_id: "entity/内部\0".into(),
                module_id: "module/α".into(),
                kind: "artifact\"mesh".into(),
                label: label.map(str::to_owned),
                anchor: anchor.clone(),
            };
            assert_original_encodings(&entity);
        }
    }
    assert_original_encodings(&ModuleVisualEntity::default());
}

#[test]
fn original_visual_adjacent_tag_and_field_order() {
    let anchors = [
        (
            ModuleVisualAnchor::Agent {
                agent_id: "a".into(),
            },
            r#"{"type":"agent","data":{"agent_id":"a"}}"#,
        ),
        (
            ModuleVisualAnchor::Location {
                location_id: "p".into(),
            },
            r#"{"type":"location","data":{"location_id":"p"}}"#,
        ),
        (
            ModuleVisualAnchor::Absolute {
                pos: GeoPos::new(1, -2, 3),
            },
            r#"{"type":"absolute","data":{"pos":{"x_cm":1,"y_cm":-2,"z_cm":3}}}"#,
        ),
    ];
    for (anchor, expected) in anchors {
        assert_eq!(serde_json::to_string(&anchor).unwrap(), expected);
        let value: ModuleVisualAnchor = serde_json::from_str(expected).unwrap();
        assert_eq!(value, anchor);
    }
    let value = ModuleVisualEntity {
        entity_id: "e".into(),
        module_id: "m".into(),
        kind: "k".into(),
        label: None,
        anchor: ModuleVisualAnchor::Agent {
            agent_id: "a".into(),
        },
    };
    assert_eq!(
        serde_json::to_string(&value).unwrap(),
        r#"{"entity_id":"e","module_id":"m","kind":"k","label":null,"anchor":{"type":"agent","data":{"agent_id":"a"}}}"#
    );
}

#[test]
fn original_missing_fields_keep_defaults() {
    assert_eq!(
        serde_json::from_str::<ModuleVisualEntity>("{}").unwrap(),
        ModuleVisualEntity::default()
    );
    let entity: ModuleVisualEntity =
        serde_json::from_str(r#"{"entity_id":" e ","kind":" ","label":" "}"#).unwrap();
    assert_eq!(
        entity.sanitized(),
        ModuleVisualEntity {
            entity_id: "e".into(),
            ..ModuleVisualEntity::default()
        }
    );
}

#[test]
fn borrowed_serialization_stops_at_first_field_error() {
    use std::cell::Cell;
    struct Field<'a> {
        seen: &'a Cell<usize>,
        fail: bool,
    }
    impl Serialize for Field<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.seen.set(self.seen.get() + 1);
            if self.fail {
                return Err(serde::ser::Error::custom("field refused"));
            }
            serializer.serialize_str("field")
        }
    }
    let seen = Cell::new(0);
    let valid = Field {
        seen: &seen,
        fail: false,
    };
    let refused = Field {
        seen: &seen,
        fail: true,
    };
    let fields = ModuleVisualEntityProjectionFieldsV1 {
        entity_id: &valid,
        module_id: &refused,
        kind: &valid,
        label: Some(&valid),
        anchor: &valid,
    };
    let error = serde_json::to_vec(&fields).unwrap_err();
    assert_eq!(error.to_string(), "field refused");
    assert_eq!(seen.get(), 2);
    seen.set(0);
    let error = ciborium::ser::into_writer(&fields, Vec::new()).unwrap_err();
    assert!(error.to_string().contains("field refused"));
    assert_eq!(seen.get(), 2);
    seen.set(0);
    let anchor = ModuleVisualAnchorProjectionDataV1::Agent { agent_id: &refused };
    assert!(serde_json::to_vec(&anchor).is_err());
    assert_eq!(seen.get(), 1);
}

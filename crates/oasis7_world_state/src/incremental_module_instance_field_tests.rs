use super::*;
use std::collections::BTreeMap;
#[test]
fn metadata_keys_and_scalar_wires_have_exact_field_binding() {
    for field in ModuleInstanceFieldV1::ALL {
        let key = ModuleInstanceKeyV1::new("map\0雪", field).unwrap();
        let mut expected = KEY.to_vec();
        expected.extend(7u16.to_be_bytes());
        expected.extend("map\0雪".as_bytes());
        expected.push(field as u8);
        assert_eq!(key.as_bytes(), expected);
        assert_eq!(
            ModuleInstanceKeyV1::from_bytes(&expected).unwrap(),
            ("map\0雪", field)
        );
        let value = match field {
            ModuleInstanceFieldV1::Target => ModuleInstanceScalarRefV1::Target(
                ModuleInstallTargetRefV1::LocationInfrastructure("  地\0 "),
            ),
            ModuleInstanceFieldV1::Active => ModuleInstanceScalarRefV1::Bool(true),
            ModuleInstanceFieldV1::InstalledAt => ModuleInstanceScalarRefV1::U64(u64::MAX),
            _ => ModuleInstanceScalarRefV1::Text("原\0文"),
        };
        let mut bytes = Vec::new();
        value.write_to(field, &mut bytes).unwrap();
        assert_eq!(value.encoded_len(field).unwrap(), bytes.len() as u64);
        assert_eq!(
            ModuleInstanceScalarRefV1::from_canonical_bytes(&bytes, field).unwrap(),
            value
        );
        let mut expected = VALUE.to_vec();
        expected.push(field as u8);
        match value {
            ModuleInstanceScalarRefV1::Text(s) => {
                expected.extend((s.len() as u64).to_be_bytes());
                expected.extend(s.as_bytes());
            }
            ModuleInstanceScalarRefV1::Target(
                ModuleInstallTargetRefV1::LocationInfrastructure(s),
            ) => {
                expected.push(1);
                expected.extend((s.len() as u64).to_be_bytes());
                expected.extend(s.as_bytes());
            }
            ModuleInstanceScalarRefV1::Bool(true) => expected.push(1),
            ModuleInstanceScalarRefV1::U64(n) => expected.extend(n.to_be_bytes()),
            _ => panic!(),
        };
        assert_eq!(bytes, expected);
        let other = if field == ModuleInstanceFieldV1::Owner {
            ModuleInstanceFieldV1::ModuleId
        } else {
            ModuleInstanceFieldV1::Owner
        };
        assert!(ModuleInstanceScalarRefV1::from_canonical_bytes(&bytes, other).is_err());
        bytes.push(0);
        assert!(ModuleInstanceScalarRefV1::from_canonical_bytes(&bytes, field).is_err());
    }
    for value in [
        ModuleInstanceScalarRefV1::Bool(false),
        ModuleInstanceScalarRefV1::Target(ModuleInstallTargetRefV1::SelfAgent),
        ModuleInstanceScalarRefV1::Target(ModuleInstallTargetRefV1::LocationInfrastructure("")),
        ModuleInstanceScalarRefV1::Text(""),
    ] {
        let field = match value {
            ModuleInstanceScalarRefV1::Bool(_) => ModuleInstanceFieldV1::Active,
            ModuleInstanceScalarRefV1::Target(_) => ModuleInstanceFieldV1::Target,
            _ => ModuleInstanceFieldV1::InstanceId,
        };
        let mut bytes = Vec::new();
        value.write_to(field, &mut bytes).unwrap();
        assert_eq!(
            ModuleInstanceScalarRefV1::from_canonical_bytes(&bytes, field).unwrap(),
            value
        );
    }
}
#[test]
fn exhaustive_instance_visitor_preserves_map_identity_target_and_all_fields() {
    let instance = crate::ModuleInstanceState {
        instance_id: "different-id".into(),
        module_id: "module".into(),
        module_version: "v2".into(),
        wasm_hash: "original-hash".into(),
        owner_agent_id: "owner".into(),
        install_target: ModuleInstallTarget::LocationInfrastructure {
            location_id: "  place  ".into(),
        },
        active: false,
        installed_at: u64::MAX,
    };
    let mut actual = BTreeMap::new();
    visit_module_instance_fields(&instance, |field, value| {
        actual.insert(field as u8, value);
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(actual.len(), 8);
    assert_eq!(actual[&1], ModuleInstanceScalarRefV1::Text("different-id"));
    assert_eq!(actual[&2], ModuleInstanceScalarRefV1::Text("module"));
    assert_eq!(actual[&3], ModuleInstanceScalarRefV1::Text("v2"));
    assert_eq!(actual[&4], ModuleInstanceScalarRefV1::Text("original-hash"));
    assert_eq!(actual[&5], ModuleInstanceScalarRefV1::Text("owner"));
    assert_eq!(
        actual[&6],
        ModuleInstanceScalarRefV1::Target(ModuleInstallTargetRefV1::LocationInfrastructure(
            "  place  "
        ))
    );
    assert_eq!(actual[&7], ModuleInstanceScalarRefV1::Bool(false));
    assert_eq!(actual[&8], ModuleInstanceScalarRefV1::U64(u64::MAX));
}
#[test]
fn malformed_keys_types_and_scalar_lengths_fail_closed() {
    let max = "x".repeat(1024 - KEY.len() - 3);
    assert_eq!(
        ModuleInstanceKeyV1::new(&max, ModuleInstanceFieldV1::Active)
            .unwrap()
            .as_bytes()
            .len(),
        1024
    );
    assert!(ModuleInstanceKeyV1::new(&(max + "x"), ModuleInstanceFieldV1::Active).is_err());
    let empty = ModuleInstanceKeyV1::new("", ModuleInstanceFieldV1::Owner).unwrap();
    assert_eq!(
        ModuleInstanceKeyV1::from_bytes(empty.as_bytes()).unwrap().0,
        ""
    );
    for bytes in [
        b"unknown".to_vec(),
        [KEY, &[0, 0, 99]].concat(),
        [KEY, &[0, 1, 255, 7]].concat(),
    ] {
        assert!(ModuleInstanceKeyV1::from_bytes(&bytes).is_err());
    }
    assert!(
        ModuleInstanceScalarRefV1::Bool(true)
            .encoded_len(ModuleInstanceFieldV1::Owner)
            .is_err()
    );
    for (field, payload) in [
        (ModuleInstanceFieldV1::Active, vec![2]),
        (ModuleInstanceFieldV1::Target, vec![2]),
        (
            ModuleInstanceFieldV1::Owner,
            [1u64.to_be_bytes().as_slice(), &[255]].concat(),
        ),
        (
            ModuleInstanceFieldV1::Owner,
            u64::MAX.to_be_bytes().to_vec(),
        ),
    ] {
        let mut bytes = VALUE.to_vec();
        bytes.push(field as u8);
        bytes.extend(payload);
        assert!(ModuleInstanceScalarRefV1::from_canonical_bytes(&bytes, field).is_err());
    }
}

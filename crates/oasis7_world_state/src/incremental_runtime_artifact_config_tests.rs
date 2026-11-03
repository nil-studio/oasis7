use super::*;

pub(super) fn module_fixture() -> ModuleManifest {
    ModuleManifest {
        module_id: "internal@id".into(),
        name: "".into(),
        version: "v@1".into(),
        kind: ModuleKind::Pure,
        role: ModuleRole::AgentInternal,
        wasm_hash: "raw".into(),
        interface_version: "".into(),
        exports: vec!["same".into(), "same".into()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["repeat".into(), "repeat".into()],
            action_kinds: vec![],
            stage: None,
            filters: Some(serde_json::Value::Null),
        }],
        required_caps: vec![],
        abi_contract: ModuleAbiContract {
            abi_version: Some(0),
            input_schema: Some(String::new()),
            output_schema: None,
            cap_slots: std::collections::BTreeMap::from([(String::new(), String::new())]),
            policy_hooks: vec![],
            gameplay: Some(GameplayContract {
                kind: GameplayModuleKind::Meta,
                game_modes: vec![],
                min_players: u16::MAX,
                max_players: Some(0),
            }),
            declarations: ModuleSchemaDeclarations {
                commands: vec![ModuleCommandDeclaration {
                    namespace: String::new(),
                    name: String::new(),
                    schema_version: u32::MAX,
                    schema_hash: String::new(),
                    max_payload_bytes: u64::MAX,
                }],
            },
        },
        artifact_identity: Some(ModuleArtifactIdentity::default()),
        limits: ModuleLimits::unbounded(),
    }
}

#[test]
fn borrowed_projection_retains_duplicate_nested_ordinals_and_options() {
    let source = module_fixture();
    let mut rows = Vec::new();
    visit_artifact_module_manifest_v1(&source, |address, payload| {
        schema::validate(schema::BodyKind::Module, address, payload).unwrap();
        rows.push((address, payload));
        Ok::<_, ()>(())
    })
    .unwrap();
    for (index, (address, _)) in rows.iter().enumerate() {
        assert!(
            !rows[..index].iter().any(|(other, _)| other == address),
            "duplicate address: {address:?}"
        );
    }
    let primitive = |section, ordinal, inner_ordinal, column| {
        rows.iter().find_map(|(address, value)| {
            (address.section == section
                && address.ordinal == ordinal
                && address.inner_ordinal == inner_ordinal
                && address.column == column)
                .then_some(value)
        })
    };
    assert!(matches!(
        primitive("subscriptions.event_kinds", Some(0), Some(0), "value"),
        Some(ArtifactConfigPayloadV1::Primitive(
            MemberPayloadRefV1::Text("repeat")
        ))
    ));
    assert!(matches!(
        primitive("subscriptions.event_kinds", Some(0), Some(1), "value"),
        Some(ArtifactConfigPayloadV1::Primitive(
            MemberPayloadRefV1::Text("repeat")
        ))
    ));
    assert!(matches!(
        primitive("subscriptions", Some(0), None, "stage"),
        Some(ArtifactConfigPayloadV1::Primitive(MemberPayloadRefV1::Unit))
    ));
    assert!(matches!(
        primitive("subscriptions", Some(0), None, "filters"),
        Some(ArtifactConfigPayloadV1::Json(serde_json::Value::Null))
    ));
    assert!(matches!(
        primitive("abi_contract", None, None, "input_schema"),
        Some(ArtifactConfigPayloadV1::Primitive(
            MemberPayloadRefV1::Text("")
        ))
    ));
    assert!(matches!(
        primitive("abi_contract", None, None, "output_schema"),
        Some(ArtifactConfigPayloadV1::Primitive(MemberPayloadRefV1::Unit))
    ));
    assert!(matches!(
        primitive(
            "abi_contract.declarations.commands",
            Some(0),
            None,
            "schema_version"
        ),
        Some(ArtifactConfigPayloadV1::Primitive(MemberPayloadRefV1::U64(
            4294967295
        )))
    ));
}

#[test]
fn callback_failure_stops_before_next_source_field() {
    let mut calls = 0;
    let result = visit_artifact_module_limits_v1(&ModuleLimits::unbounded(), |_, _| {
        calls += 1;
        Err::<(), _>("bounded caller work")
    });
    assert_eq!(result, Err("bounded caller work"));
    assert_eq!(calls, 1);
}

#[test]
fn closed_columns_reject_wrong_width_enum_and_address_shape() {
    let address = ArtifactConfigAddressV1 {
        section: "limits",
        ordinal: None,
        inner_ordinal: None,
        member: None,
        column: "max_call_rate",
    };
    let value = |value| ArtifactConfigPayloadV1::Primitive(MemberPayloadRefV1::U64(value));
    assert!(
        schema::validate(
            schema::BodyKind::Limits,
            address,
            value(u64::from(u32::MAX))
        )
        .is_ok()
    );
    assert!(
        schema::validate(
            schema::BodyKind::Limits,
            address,
            value(u64::from(u32::MAX) + 1)
        )
        .is_err()
    );
    assert!(
        schema::validate(
            schema::BodyKind::Limits,
            ArtifactConfigAddressV1 {
                ordinal: Some(0),
                ..address
            },
            value(0)
        )
        .is_err()
    );
    let role = ArtifactConfigAddressV1 {
        section: "manifest",
        column: "role",
        ..address
    };
    assert!(
        schema::validate(
            schema::BodyKind::Module,
            role,
            ArtifactConfigPayloadV1::Primitive(MemberPayloadRefV1::Text("agent_internal"))
        )
        .is_ok()
    );
    assert!(
        schema::validate(
            schema::BodyKind::Module,
            role,
            ArtifactConfigPayloadV1::Primitive(MemberPayloadRefV1::Text("AgentInternal"))
        )
        .is_err()
    );
    assert!(
        schema::validate(
            schema::BodyKind::Manifest,
            role,
            ArtifactConfigPayloadV1::Primitive(MemberPayloadRefV1::Text("rule"))
        )
        .is_err()
    );
}

#[test]
fn relative_key_probe_preserves_both_ordinals_and_streams_original_members() {
    use crate::incremental_semantic_key::write_semantic_key_v1;
    use std::io::Write;
    let memory = oasis7_proto::memory_budget::MemoryBudget::new(1 << 20, 0).unwrap();
    let _admission = memory
        .reserve(32 << 10, oasis7_proto::memory_budget::MemoryClass::Ordinary)
        .unwrap();
    let long_key = "原key".repeat(1000);
    for address in [
        ArtifactConfigAddressV1 {
            section: "subscriptions.event_kinds",
            ordinal: Some(0),
            inner_ordinal: Some(u64::MAX),
            member: None,
            column: "value",
        },
        ArtifactConfigAddressV1 {
            section: "abi_contract.cap_slots",
            ordinal: None,
            inner_ordinal: None,
            member: Some(long_key.as_str()),
            column: "value",
        },
    ] {
        let mut bytes = Vec::with_capacity(16 << 10);
        write_semantic_key_v1(&address.semantic(), &mut bytes).unwrap();
        for chunk in [1, 17, bytes.len()] {
            let mut sink = std::io::sink();
            let mut probe = key_read::Probe::new(&mut sink);
            for part in bytes.chunks(chunk) {
                probe.write_all(part).unwrap();
            }
            let actual = probe.finish().unwrap();
            assert_eq!(
                (
                    actual.section,
                    actual.column,
                    actual.ordinal,
                    actual.inner_ordinal,
                    actual.member.is_some()
                ),
                (
                    address.section,
                    address.column,
                    address.ordinal,
                    address.inner_ordinal,
                    address.member.is_some()
                )
            );
        }
    }
    let mut bytes = Vec::with_capacity(1024);
    // Preserve the unique existing key producer; only the invalid presence
    // tuple is supplied, so the decoder cannot normalize None to ordinal 1.
    write_semantic_key_v1(
        &(
            ("exports", "value"),
            ((0u64, 1u64), ((0u64, 0u64), (0u64, ""))),
        ),
        &mut bytes,
    )
    .unwrap();
    let mut sink = std::io::sink();
    let mut probe = key_read::Probe::new(&mut sink);
    assert!(probe.write_all(&bytes).is_err());
    assert!(probe.finish().is_err());
}

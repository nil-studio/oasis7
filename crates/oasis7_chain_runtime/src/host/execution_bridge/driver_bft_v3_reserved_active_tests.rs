//! Default-stack phases keep one owned window on the heap throughout.
use super::*;
pub(super) fn run(case: &mut ActiveExerciseV1<'_>) {
    let (mut window, base_schedule) = open_window(case).unwrap();
    assert_migration_codec(case);
    let pending = window
        .pending_candidate()
        .unwrap()
        .expect("original decided T+1 must survive");
    let certificate = certify_one(window.schedule(), &pending.public_value);
    rolling::reserve_certificate(case.lease, &certificate);
    window
        .append_certified(certificate.clone(), deadline())
        .unwrap();
    rolling::confirm_applied(case.lease, &window, &certificate);
    let outcome = exercise_reserved_node_consumer(
        window,
        case.binding,
        case.lease,
        &certificate,
        &base_schedule,
    );
    let (cold, _) = open_window(case).expect("actual reserved suffix must cold reopen");
    assert!(
        cold.world()
            .state()
            .agents
            .contains_key("active-node-reserved-business")
    );
    assert_eq!(
        cold.publication_accounting_for_test().unwrap(),
        outcome.accounting
    );
    drop(cold);
    let segment = case
        .root
        .path()
        .join("ledger/successor-active-v1")
        .join(case.record.request.intent.body.checkpoint.digest().unwrap())
        .join("formal-bft-header-window-v1/node-private-v1");
    let mut metadata = None;
    for entry in std::fs::read_dir(&segment).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("candidate-")
        {
            continue;
        }
        let value: serde_cbor::Value =
            serde_cbor::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let serde_cbor::Value::Map(fields) = value else {
            panic!("Data map missing")
        };
        if let Some(serde_cbor::Value::Text(reference)) = fields.get(&serde_cbor::Value::Text(
            "publication_reservation_ref".into(),
        )) {
            metadata = Some(
                case.store
                    .root()
                    .join("blobs")
                    .join(format!("{reference}.blob")),
            );
        }
    }
    let path = metadata.expect("fresh Active Some metadata missing");
    let bytes = std::fs::read(&path).unwrap();
    let permissions = std::fs::metadata(&path).unwrap().permissions();
    std::fs::remove_file(&path).unwrap();
    assert!(
        open_window(case).is_err(),
        "cold active cannot reinterpret missing Some as legacy"
    );
    std::fs::write(&path, &bytes).unwrap();
    std::fs::set_permissions(&path, permissions).unwrap();
    let (cold, _) =
        open_window(case).expect("restored original metadata must recover same active suffix");
    assert_eq!(
        cold.publication_accounting_for_test().unwrap(),
        outcome.accounting
    );
    let mut retry = cold.into_node_hook_for_test(case.binding).unwrap();
    retry
        .hook()
        .configure_bft_v3_execution(case.binding)
        .unwrap();
    retry
        .hook()
        .commit_bft_v3_certificate(outcome.original)
        .unwrap();
    let result = retry
        .hook()
        .commit_bft_v3_certificate(outcome.fresh)
        .unwrap();
    assert_eq!(result, outcome.result);
    assert_eq!(
        retry.accounting().unwrap(),
        outcome.accounting,
        "cold exact original QC retry cannot reset or duplicate promise"
    );
}
#[inline(never)]
fn open_window(
    case: &mut ActiveExerciseV1<'_>,
) -> Result<
    (
        Box<FormalBftHeaderActiveExecutionV1>,
        BftHeaderHeightScheduleV3,
    ),
    String,
> {
    let active = case
        .lease
        .verified_active_window()
        .map_err(|e| e.to_string())?
        .ok_or("active capability missing")?;
    let installed = case
        .original
        .reconstruct_installed_pending(case.binding, case.pin, case.record)
        .map_err(|e| e.to_string())?;
    let base = case
        .verifier
        .recover_private_base(case.record, case.original, deadline())?;
    let window = Box::new(FormalBftHeaderActiveExecutionV1::open(
        &case.root.path().join("ledger"),
        case.store.clone(),
        base,
        &active,
        case.binding,
        case.pin,
        &installed,
        deadline(),
    )?);
    Ok((window, active.base_schedule().clone()))
}

fn assert_migration_codec(case: &ActiveExerciseV1<'_>) {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LegacyData {
        schema_version: u32,
        #[serde(rename = "domain")]
        _domain: serde::de::IgnoredAny,
        #[serde(rename = "origin")]
        _origin: serde::de::IgnoredAny,
        #[serde(rename = "ledger_origin_digest")]
        _ledger_origin: serde::de::IgnoredAny,
        #[serde(rename = "material_identity")]
        _material: serde::de::IgnoredAny,
        #[serde(rename = "instance_id")]
        _instance: serde::de::IgnoredAny,
        #[serde(rename = "height")]
        _height: serde::de::IgnoredAny,
        #[serde(rename = "value")]
        _value: serde::de::IgnoredAny,
        #[serde(rename = "actions_cbor")]
        _actions: serde::de::IgnoredAny,
        #[serde(rename = "references")]
        _references: serde::de::IgnoredAny,
        #[serde(rename = "publication_reservation_ref")]
        _publication: Option<serde::de::IgnoredAny>,
    }
    let segment = case
        .root
        .path()
        .join("ledger/successor-active-v1")
        .join(case.record.request.intent.body.checkpoint.digest().unwrap())
        .join("formal-bft-header-window-v1/node-private-v1");
    for entry in std::fs::read_dir(segment).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("candidate-")
        {
            continue;
        }
        let bytes = std::fs::read(path).unwrap();
        let value: serde_cbor::Value = serde_cbor::from_slice(&bytes).unwrap();
        let serde_cbor::Value::Map(mut fields) = value else {
            panic!("migration Data map missing")
        };
        if fields
            .remove(&serde_cbor::Value::Text(
                "publication_original_data_ref".into(),
            ))
            .is_some()
        {
            assert!(
                serde_cbor::from_slice::<LegacyData>(&bytes).is_err(),
                "old deny-unknown codec must reject migrated Some"
            );
            let stripped: LegacyData =
                serde_cbor::value::from_value(serde_cbor::Value::Map(fields)).unwrap();
            assert!(
                !matches!(stripped.schema_version, 3..=5),
                "even an ignored migration field cannot reopen old legacy schema"
            );
        }
    }
}

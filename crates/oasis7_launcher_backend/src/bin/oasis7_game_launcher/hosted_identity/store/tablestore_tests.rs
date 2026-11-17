//! Loopback protocol fixture: verifies wire transaction boundaries, not cloud capability.
use super::*;
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Default)]
struct State {
    committed: BTreeMap<String, Vec<u8>>,
    active: Option<(String, BTreeMap<String, Vec<u8>>)>,
    lost_commit: bool,
    operations: Vec<String>,
}
struct Fixture {
    endpoint: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
fn row(bytes: Vec<u8>) -> Row {
    aliyun_tablestore_rs::data::GetRowResponse::try_from(pb::GetRowResponse {
        row: bytes,
        ..Default::default()
    })
    .unwrap()
    .row
    .unwrap()
}
fn key(mut bytes: Vec<u8>) -> String {
    if bytes.len() > 3 && bytes[bytes.len() - 3] == 8 {
        // SDK read decoder omits delete-marker support; normalize only this fixture.
        let marker = bytes.len() - 3;
        bytes.remove(marker);
        let last = bytes.len() - 1;
        bytes[last] ^= aliyun_tablestore_rs::crc8::crc_u8(0, 1);
    }
    match row(bytes).get_primary_key_value("record_key").unwrap() {
        aliyun_tablestore_rs::model::PrimaryKeyValue::String(s) => s.clone(),
        _ => panic!("key type"),
    }
}
fn send_response(stream: &mut TcpStream, operation: &str, status: u16, body: Vec<u8>) {
    let headers = BTreeMap::from([
        (
            "x-ots-date".into(),
            ots_timestamp(time::OffsetDateTime::now_utc()).unwrap(),
        ),
        (
            "x-ots-contentmd5".into(),
            STANDARD.encode(md5::compute(&body).0),
        ),
        ("x-ots-requestid".into(), "fixture".into()),
        ("x-ots-contenttype".into(), "application/x-protobuf".into()),
    ]);
    let auth = sign(
        "fixture-secret",
        &format!("{}\n/{operation}", canonical(&headers)),
    )
    .unwrap();
    write!(stream,"HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\nAuthorization: OTS fixture-ak:{auth}\r\n",body.len()).unwrap();
    for (k, v) in headers {
        write!(stream, "{k}: {v}\r\n").unwrap();
    }
    stream.write_all(b"\r\n").unwrap();
    stream.write_all(&body).unwrap();
}
impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_state = state.clone();
        let worker_stop = stop.clone();
        let thread = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(s) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let boundary = loop {
                    let mut chunk = [0; 4096];
                    let count = stream.read(&mut chunk).unwrap();
                    assert_ne!(count, 0);
                    request.extend_from_slice(&chunk[..count]);
                    if let Some(i) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let text = std::str::from_utf8(&request[..boundary]).unwrap();
                let operation = text
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .trim_start_matches('/')
                    .to_owned();
                let mut headers: BTreeMap<String, String> = text
                    .lines()
                    .skip(1)
                    .filter_map(|s| s.split_once(':'))
                    .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().into()))
                    .collect();
                let length = headers["content-length"].parse::<usize>().unwrap();
                while request.len() < boundary + length {
                    let mut chunk = [0; 4096];
                    let count = stream.read(&mut chunk).unwrap();
                    assert_ne!(count, 0);
                    request.extend_from_slice(&chunk[..count]);
                }
                let signature = headers.remove("x-ots-signature").unwrap();
                assert_eq!(
                    signature,
                    sign(
                        "fixture-secret",
                        &format!("/{operation}\nPOST\n\n{}\n", canonical(&headers))
                    )
                    .unwrap()
                );
                let body = &request[boundary..boundary + length];
                let mut state = worker_state.lock().unwrap();
                state.operations.push(operation.clone());
                let mut status = 200;
                let response = match operation.as_str() {
                    "DescribeTable" => pb::DescribeTableResponse {
                        table_meta: pb::TableMeta {
                            table_name: "identity_v2".into(),
                            primary_key: vec![
                                pb::PrimaryKeySchema {
                                    name: "namespace".into(),
                                    r#type: 2,
                                    option: None,
                                },
                                pb::PrimaryKeySchema {
                                    name: "record_key".into(),
                                    r#type: 2,
                                    option: None,
                                },
                            ],
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .encode_to_vec(),
                    "StartLocalTransaction" => {
                        let q = pb::StartLocalTransactionRequest::decode(body).unwrap();
                        assert_eq!(row(q.key).primary_key.columns.len(), 1);
                        if state.active.is_some() {
                            status = 409;
                            pb::Error {
                                code: "OTSRowOperationConflict".into(),
                                ..Default::default()
                            }
                            .encode_to_vec()
                        } else {
                            let id = format!("tx-{}", state.operations.len());
                            state.active = Some((id.clone(), state.committed.clone()));
                            pb::StartLocalTransactionResponse { transaction_id: id }.encode_to_vec()
                        }
                    }
                    "GetRow" => {
                        let q = pb::GetRowRequest::decode(body).unwrap();
                        let (id, rows) = state.active.as_ref().unwrap();
                        assert_eq!(q.transaction_id.as_deref(), Some(id.as_str()));
                        pb::GetRowResponse {
                            row: rows.get(&key(q.primary_key)).cloned().unwrap_or_default(),
                            ..Default::default()
                        }
                        .encode_to_vec()
                    }
                    "PutRow" => {
                        let q = pb::PutRowRequest::decode(body).unwrap();
                        let (id, rows) = state.active.as_mut().unwrap();
                        assert_eq!(q.transaction_id.as_deref(), Some(id.as_str()));
                        let key = key(q.row.clone());
                        let condition = q.condition.row_existence;
                        let conflict = (condition
                            == pb::RowExistenceExpectation::ExpectNotExist as i32
                            && rows.contains_key(&key))
                            || (condition == pb::RowExistenceExpectation::ExpectExist as i32
                                && !rows.contains_key(&key));
                        if conflict {
                            status = 409;
                            pb::Error {
                                code: "OTSConditionCheckFail".into(),
                                ..Default::default()
                            }
                            .encode_to_vec()
                        } else {
                            rows.insert(key, q.row);
                            pb::PutRowResponse::default().encode_to_vec()
                        }
                    }
                    "DeleteRow" => {
                        let q = pb::DeleteRowRequest::decode(body).unwrap();
                        let (id, rows) = state.active.as_mut().unwrap();
                        assert_eq!(q.transaction_id.as_deref(), Some(id.as_str()));
                        rows.remove(&key(q.primary_key));
                        pb::DeleteRowResponse::default().encode_to_vec()
                    }
                    "GetRange" => {
                        let q = pb::GetRangeRequest::decode(body).unwrap();
                        let (id, rows) = state.active.as_ref().unwrap();
                        assert_eq!(q.transaction_id.as_deref(), Some(id.as_str()));
                        let start = key(q.inclusive_start_primary_key);
                        let end = key(q.exclusive_end_primary_key);
                        let mut encoded = vec![0x75, 0, 0, 0];
                        for (_, bytes) in
                            rows.range(start..end).take(q.limit.unwrap_or(100) as usize)
                        {
                            encoded.extend_from_slice(&bytes[4..]);
                        }
                        pb::GetRangeResponse {
                            rows: encoded,
                            ..Default::default()
                        }
                        .encode_to_vec()
                    }
                    "CommitTransaction" => {
                        let q = pb::CommitTransactionRequest::decode(body).unwrap();
                        let (id, rows) = state.active.take().unwrap();
                        assert_eq!(q.transaction_id, id);
                        state.committed = rows;
                        if state.lost_commit {
                            state.lost_commit = false;
                            continue;
                        }
                        pb::CommitTransactionResponse::default().encode_to_vec()
                    }
                    "AbortTransaction" => {
                        let q = pb::AbortTransactionRequest::decode(body).unwrap();
                        if let Some((id, _)) = state.active.take() {
                            assert_eq!(q.transaction_id, id);
                        }
                        pb::AbortTransactionResponse::default().encode_to_vec()
                    }
                    _ => panic!("unexpected {operation}"),
                };
                send_response(&mut stream, &operation, status, response);
            }
        });
        Self {
            endpoint,
            state,
            stop,
            thread: Some(thread),
        }
    }
    fn store(&self, initialize: bool) -> super::super::IdentityStore {
        let ns = IdentityNamespace {
            identity_service_id: "wire-test".into(),
            environment_id: "isolated".into(),
        };
        let rp = RpConfig {
            rp_id: "example.test".into(),
            allowed_origins: vec!["https://example.test".into()],
            version: 1,
            identity_instance_epoch: 1,
        };
        super::super::IdentityStore::open_tablestore(
            TablestoreConfig {
                endpoint: self.endpoint.clone(),
                instance: "fixture-instance".into(),
                access_key_id: "fixture-ak".into(),
                access_key_secret: "fixture-secret".into(),
                sts_token: None,
                table: "identity_v2".into(),
                initialize,
            },
            ns,
            rp,
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let result = self.thread.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}
#[test]
fn real_wire_adapter_aborts_partial_writes_and_reconciles_unknown_commit() {
    let f = Fixture::new();
    let mut s = f.store(true);
    let result = s.transaction(|tx| {
        tx.create("account:a", "account", Some("a"), None, &"first")?;
        tx.create("account:a", "account", Some("a"), None, &"duplicate")
    });
    assert!(matches!(result, Err(StoreError::Conflict)));
    assert!(
        s.transaction(|tx| tx.get::<String>("account:a"))
            .unwrap()
            .is_none()
    );
    f.state.lock().unwrap().lost_commit = true;
    let result = s.transaction(|tx| {
        tx.create("account:a", "account", Some("a"), None, &"first")?;
        tx.create("receipt:once", "receipt", Some("a"), Some(100), &true)
    });
    assert!(matches!(result, Err(StoreError::OutcomeUnknown)));
    assert!(
        s.transaction(|tx| tx.get::<bool>("receipt:once"))
            .unwrap()
            .unwrap()
            .value
    );
    assert_eq!(
        s.transaction(|tx| tx.list::<String>("account", Some("a"), None, 10))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(s.cleanup_expired(100).unwrap(), 1);
    assert!(
        s.transaction(|tx| tx.get::<String>("account:a"))
            .unwrap()
            .is_some()
    );
    let operations = &f.state.lock().unwrap().operations;
    assert!(operations.iter().any(|o| o == "AbortTransaction"));
    assert!(!operations.iter().any(|o| o == "BatchWriteRow"));
}
#[test]
fn request_signature_matches_official_python_primitive() {
    let headers = BTreeMap::from([
        ("x-ots-apiversion".into(), "2015-12-31".into()),
        ("x-ots-accesskeyid".into(), "fixture-ak".into()),
        ("x-ots-instancename".into(), "fixture-instance".into()),
    ]);
    let text = format!("/StartLocalTransaction\nPOST\n\n{}\n", canonical(&headers));
    assert_eq!(
        sign("fixture-secret", &text).unwrap(),
        "t6w4jfDj1KiTnYwpLIvQ9meEOms="
    );
}

#[test]
fn ots_timestamp_preserves_signed_utc_millisecond_bytes() {
    use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

    let leap = PrimitiveDateTime::new(
        Date::from_calendar_date(2024, Month::February, 29).unwrap(),
        Time::from_hms_nano(12, 34, 56, 123_999_999).unwrap(),
    )
    .assume_utc();
    assert_eq!(ots_timestamp(leap).unwrap(), "2024-02-29T12:34:56.123Z");
    assert_eq!(
        ots_timestamp(leap.to_offset(UtcOffset::from_hms(8, 0, 0).unwrap())).unwrap(),
        "2024-02-29T12:34:56.123Z"
    );
    for (nanos, expected) in [
        (0, "1970-01-01T00:00:00.000Z"),
        (999_999, "1970-01-01T00:00:00.000Z"),
        (1_000_000, "1970-01-01T00:00:00.001Z"),
        (-1, "1969-12-31T23:59:59.999Z"),
    ] {
        assert_eq!(
            ots_timestamp(time::OffsetDateTime::from_unix_timestamp_nanos(nanos).unwrap()).unwrap(),
            expected
        );
    }
    let headers = BTreeMap::from([
        ("x-ots-date".into(), ots_timestamp(leap).unwrap()),
        ("x-ots-instancename".into(), "fixture".into()),
    ]);
    let canonical = canonical(&headers);
    assert_eq!(
        canonical,
        "x-ots-date:2024-02-29T12:34:56.123Z\nx-ots-instancename:fixture"
    );
    assert_eq!(
        sign("fixture-secret", &format!("/GetRow\nPOST\n\n{canonical}\n")).unwrap(),
        "azPrBBPxDQ7SY0bUW/f+iaa4azs="
    );
}

#[test]
fn ots_timestamp_rejects_non_protocol_years() {
    use time::{Date, Month};
    for (year, expected) in [
        (0, "0000-01-01T00:00:00.000Z"),
        (9999, "9999-01-01T00:00:00.000Z"),
    ] {
        let at = Date::from_calendar_date(year, Month::January, 1)
            .unwrap()
            .midnight()
            .assume_utc();
        assert_eq!(ots_timestamp(at).unwrap(), expected);
    }
    let at = Date::from_calendar_date(-1, Month::January, 1)
        .unwrap()
        .midnight()
        .assume_utc();
    assert!(ots_timestamp(at).is_err());
}

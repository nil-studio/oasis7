//! Native adapter for the SDK's unexposed local-transaction operations.
//! Protocol and V2 signing follow the official Aliyun Python SDK auth.py/protocol.py.
//! No automatic retries: unknown commit must be reconciled through the operation receipt.
use super::super::model::{IdentityNamespace, RpConfig};
use super::{StoreError, Versioned, sql_integer};
use aliyun_tablestore_rs::{
    data::{DeleteRowRequest, GetRangeRequest, GetRowRequest, PutRowRequest},
    model::{ColumnValue, Row},
    protos as pb,
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use hmac_legacy::{Hmac, Mac};
use prost::Message;
use serde::{Serialize, de::DeserializeOwned};
use sha1::Sha1;
use std::{
    collections::BTreeMap,
    io::Read,
    sync::Arc,
    time::{Duration, Instant},
};

pub struct TablestoreConfig {
    pub endpoint: String,
    pub instance: String,
    pub access_key_id: String,
    pub access_key_secret: String,
    pub sts_token: Option<String>,
    pub table: String,
    pub initialize: bool,
}
struct Transport {
    config: TablestoreConfig,
    client: reqwest::blocking::Client,
}
#[derive(Clone)]
pub struct TablestoreStore {
    transport: Arc<Transport>,
    namespace: String,
}
pub struct RemoteTransaction {
    store: TablestoreStore,
    id: String,
    deadline: Instant,
    finished: bool,
    written: usize,
}
#[derive(Clone, Serialize, serde::Deserialize)]
struct Metadata {
    schema: u64,
    namespace: IdentityNamespace,
    rp: RpConfig,
}
#[derive(Clone, Serialize, serde::Deserialize)]
struct StoredRecord {
    key: String,
    kind: String,
    account_id: Option<String>,
    version: u64,
    expires: Option<u64>,
    payload: Vec<u8>,
}
fn protocol_error() -> StoreError {
    StoreError::Unavailable("Tablestore protocol or transaction capability unavailable".into())
}
fn sign(secret: &str, message: &str) -> Result<String, StoreError> {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).map_err(|_| protocol_error())?;
    mac.update(message.as_bytes());
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}
fn canonical(headers: &BTreeMap<String, String>) -> String {
    headers
        .iter()
        .filter(|(k, _)| k.starts_with("x-ots-"))
        .map(|(k, v)| format!("{}:{}", k.to_ascii_lowercase(), v.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}
impl Transport {
    fn request<Q: Message, R: Message + Default>(
        &self,
        operation: &str,
        request: &Q,
        deadline: Instant,
    ) -> Result<R, StoreError> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(StoreError::Busy)?;
        let body = request.encode_to_vec();
        if body.len() > 4 * 1024 * 1024 {
            return Err(StoreError::Rejected(
                "identity_transaction_too_large".into(),
            ));
        }
        let mut headers = BTreeMap::from([
            (
                "x-ots-date".into(),
                chrono::Utc::now()
                    .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                    .to_string(),
            ),
            ("x-ots-apiversion".into(), "2015-12-31".into()),
            (
                "x-ots-accesskeyid".into(),
                self.config.access_key_id.clone(),
            ),
            ("x-ots-instancename".into(), self.config.instance.clone()),
            (
                "x-ots-contentmd5".into(),
                STANDARD.encode(md5::compute(&body).0),
            ),
        ]);
        if let Some(token) = &self.config.sts_token {
            headers.insert("x-ots-ststoken".into(), token.clone());
        }
        let signature = sign(
            &self.config.access_key_secret,
            &format!("/{operation}\nPOST\n\n{}\n", canonical(&headers)),
        )?;
        headers.insert("x-ots-signature".into(), signature);
        let mut request = self
            .client
            .post(format!("{}/{operation}", self.config.endpoint))
            .timeout(remaining.min(Duration::from_secs(5)))
            .body(body);
        for (k, v) in headers {
            request = request.header(k, v);
        }
        let response = request.send().map_err(|_| protocol_error())?;
        let status = response.status();
        let response_headers = response
            .headers()
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.as_str().to_owned(),
                    v.to_str().map_err(|_| protocol_error())?.to_owned(),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, StoreError>>()?;
        let mut body = Vec::new();
        response
            .take(5 * 1024 * 1024 + 1)
            .read_to_end(&mut body)
            .map_err(|_| protocol_error())?;
        if body.len() > 5 * 1024 * 1024 {
            return Err(protocol_error());
        }
        if let Some(hash) = response_headers.get("x-ots-contentmd5") {
            if hash != &STANDARD.encode(md5::compute(&body).0) {
                return Err(protocol_error());
            }
        } else if status.is_success() {
            return Err(protocol_error());
        }
        if status.is_success() {
            let authorization = response_headers
                .get("authorization")
                .ok_or_else(protocol_error)?;
            let expected = format!("OTS {}:", self.config.access_key_id);
            let signature = STANDARD
                .decode(
                    authorization
                        .strip_prefix(&expected)
                        .ok_or_else(protocol_error)?,
                )
                .map_err(|_| protocol_error())?;
            let mut mac = Hmac::<Sha1>::new_from_slice(self.config.access_key_secret.as_bytes())
                .map_err(|_| protocol_error())?;
            mac.update(format!("{}\n/{operation}", canonical(&response_headers)).as_bytes());
            mac.verify_slice(&signature).map_err(|_| protocol_error())?;
        }
        if !status.is_success() {
            let error = pb::Error::decode(body.as_slice()).map_err(|_| protocol_error())?;
            return Err(match error.code.as_str() {
                "OTSRowOperationConflict" | "OTSSessionBusy" => StoreError::Busy,
                "OTSConditionCheckFail" => StoreError::Conflict,
                _ => protocol_error(),
            });
        }
        R::decode(body.as_slice()).map_err(|_| protocol_error())
    }
}
impl TablestoreStore {
    pub fn open(
        mut config: TablestoreConfig,
        namespace: IdentityNamespace,
        rp: RpConfig,
    ) -> Result<Self, StoreError> {
        let endpoint = url::Url::parse(&config.endpoint).map_err(|_| protocol_error())?;
        #[cfg(not(test))]
        if endpoint.scheme() != "https" {
            return Err(protocol_error());
        }
        if !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || endpoint.path() != "/"
            || config.table.is_empty()
            || config.access_key_id.is_empty()
            || config.access_key_secret.is_empty()
            || config.instance.is_empty()
        {
            return Err(protocol_error());
        }
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| protocol_error())?;
        config.endpoint = config.endpoint.trim_end_matches('/').to_owned();
        let initialize = config.initialize;
        let store = Self {
            transport: Arc::new(Transport { config, client }),
            namespace: namespace.key(),
        };
        let described: pb::DescribeTableResponse = store.transport.request(
            "DescribeTable",
            &pb::DescribeTableRequest {
                table_name: store.transport.config.table.clone(),
            },
            Instant::now() + Duration::from_secs(5),
        )?;
        let keys = &described.table_meta.primary_key;
        if keys.len() != 2
            || keys[0].name != "namespace"
            || keys[1].name != "record_key"
            || keys
                .iter()
                .any(|k| k.r#type != pb::PrimaryKeyType::String as i32 || k.option.is_some())
        {
            return Err(StoreError::Rejected(
                "identity_table_schema_mismatch".into(),
            ));
        }
        let expected = Metadata {
            schema: 2,
            namespace,
            rp,
        };
        // A table-wide anchor prevents opening a second empty namespace in the
        // same table when a deployment environment or service ID is mistyped.
        let mut anchor_store = store.clone();
        anchor_store.namespace = "__oasis7_identity_table_v2__".into();
        let mut anchor = anchor_store.begin()?;
        if let Some(meta) = anchor.get::<Metadata>("metadata:table")? {
            if serde_json::to_value(meta.value).map_err(|_| StoreError::Corrupt)?
                != serde_json::to_value(&expected).map_err(|_| StoreError::Corrupt)?
            {
                return Err(StoreError::Rejected(
                    "identity_namespace_or_rp_mismatch".into(),
                ));
            }
        } else {
            if !initialize {
                return Err(StoreError::Rejected(
                    "identity_table_metadata_missing".into(),
                ));
            }
            anchor.create("metadata:table", "metadata", None, None, &expected)?;
        }
        anchor.commit()?;
        let mut transaction = store.begin()?;
        if let Some(meta) = transaction.get::<Metadata>("metadata:identity")? {
            if serde_json::to_value(meta.value).map_err(|_| StoreError::Corrupt)?
                != serde_json::to_value(&expected).map_err(|_| StoreError::Corrupt)?
            {
                return Err(StoreError::Rejected(
                    "identity_namespace_or_rp_mismatch".into(),
                ));
            }
        } else {
            if !initialize {
                return Err(StoreError::Rejected(
                    "identity_table_metadata_missing".into(),
                ));
            }
            transaction.create("metadata:identity", "metadata", None, None, &expected)?;
        }
        transaction.commit()?;
        Ok(store)
    }
    pub fn begin(&self) -> Result<RemoteTransaction, StoreError> {
        let partition: pb::GetRowRequest = GetRowRequest::new(&self.transport.config.table)
            .primary_key_column_string("namespace", &self.namespace)
            .into();
        let deadline = Instant::now() + Duration::from_secs(40);
        let response: pb::StartLocalTransactionResponse = self.transport.request(
            "StartLocalTransaction",
            &pb::StartLocalTransactionRequest {
                table_name: self.transport.config.table.clone(),
                key: partition.primary_key,
            },
            deadline,
        )?;
        if response.transaction_id.is_empty() || response.transaction_id.len() > 4096 {
            return Err(protocol_error());
        }
        Ok(RemoteTransaction {
            store: self.clone(),
            id: response.transaction_id,
            deadline,
            finished: false,
            written: 0,
        })
    }
}
impl RemoteTransaction {
    fn get_record(&self, key: &str) -> Result<Option<StoredRecord>, StoreError> {
        let request: pb::GetRowRequest = GetRowRequest::new(&self.store.transport.config.table)
            .primary_key_column_string("namespace", &self.store.namespace)
            .primary_key_column_string("record_key", format!("r:{key}"))
            .transaction_id(&self.id)
            .into();
        let response: pb::GetRowResponse =
            self.store
                .transport
                .request("GetRow", &request, self.deadline)?;
        let decoded: aliyun_tablestore_rs::data::GetRowResponse =
            response.try_into().map_err(|_| protocol_error())?;
        decoded.row.map(decode_record).transpose()
    }
    pub(super) fn record_kind(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self.get_record(key)?.map(|record| record.kind))
    }
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<Versioned<T>>, StoreError> {
        self.get_record(key)?
            .map(|record| {
                Ok(Versioned {
                    row_version: record.version,
                    value: serde_json::from_slice(&record.payload)
                        .map_err(|_| StoreError::Corrupt)?,
                })
            })
            .transpose()
    }
    fn write_row(
        &mut self,
        key: &str,
        record: &StoredRecord,
        expect: pb::RowExistenceExpectation,
    ) -> Result<(), StoreError> {
        let payload = serde_json::to_vec(record).map_err(|_| StoreError::Corrupt)?;
        self.written = self
            .written
            .saturating_add(payload.len() + key.len() + 1024);
        if self.written > 3 * 1024 * 1024 {
            return Err(StoreError::Rejected(
                "identity_transaction_too_large".into(),
            ));
        }
        let row = Row::new()
            .primary_key_column_string("namespace", &self.store.namespace)
            .primary_key_column_string("record_key", key)
            .column_blob("record", payload);
        let request: pb::PutRowRequest = PutRowRequest::new(&self.store.transport.config.table)
            .row(row)
            .row_condition(expect)
            .transaction_id(&self.id)
            .into();
        let _: pb::PutRowResponse =
            self.store
                .transport
                .request("PutRow", &request, self.deadline)?;
        Ok(())
    }
    fn delete_row(&mut self, key: &str) -> Result<(), StoreError> {
        let request: pb::DeleteRowRequest =
            DeleteRowRequest::new(&self.store.transport.config.table)
                .primary_key_column_string("namespace", &self.store.namespace)
                .primary_key_column_string("record_key", key)
                .transaction_id(&self.id)
                .into();
        let _: pb::DeleteRowResponse =
            self.store
                .transport
                .request("DeleteRow", &request, self.deadline)?;
        Ok(())
    }
    fn index_keys(record: &StoredRecord) -> Vec<String> {
        let kind = URL_SAFE_NO_PAD.encode(&record.kind);
        let mut keys = vec![format!("i:g:{kind}:{}", record.key)];
        if let Some(account) = &record.account_id {
            keys.push(format!(
                "i:a:{}:{kind}:{}",
                URL_SAFE_NO_PAD.encode(account),
                record.key
            ));
        }
        if let Some(expires) = record.expires {
            keys.push(format!("i:e:{expires:020}:{}", record.key));
        }
        keys
    }
    fn write_record(
        &mut self,
        record: &StoredRecord,
        expect: pb::RowExistenceExpectation,
    ) -> Result<(), StoreError> {
        self.write_row(&format!("r:{}", record.key), record, expect)?;
        for key in Self::index_keys(record) {
            self.write_row(&key, record, pb::RowExistenceExpectation::Ignore)?;
        }
        Ok(())
    }
    pub fn create<T: Serialize>(
        &mut self,
        key: &str,
        kind: &str,
        account_id: Option<&str>,
        expires: Option<u64>,
        payload: &T,
    ) -> Result<(), StoreError> {
        let record = StoredRecord {
            key: key.into(),
            kind: kind.into(),
            account_id: account_id.map(str::to_owned),
            version: 1,
            expires,
            payload: serde_json::to_vec(payload).map_err(|_| StoreError::Corrupt)?,
        };
        self.write_record(&record, pb::RowExistenceExpectation::ExpectNotExist)
    }
    pub fn replace<T: Serialize>(
        &mut self,
        key: &str,
        expected: u64,
        kind: &str,
        account_id: Option<&str>,
        expires: Option<u64>,
        payload: &T,
    ) -> Result<(), StoreError> {
        let old = self.get_record(key)?.ok_or(StoreError::Conflict)?;
        if old.version != expected {
            return Err(StoreError::Conflict);
        }
        for index in Self::index_keys(&old) {
            self.delete_row(&index)?;
        }
        let record = StoredRecord {
            key: key.into(),
            kind: kind.into(),
            account_id: account_id.map(str::to_owned),
            version: expected.checked_add(1).ok_or(StoreError::Corrupt)?,
            expires,
            payload: serde_json::to_vec(payload).map_err(|_| StoreError::Corrupt)?,
        };
        self.write_record(&record, pb::RowExistenceExpectation::ExpectExist)
    }
    pub fn delete(&mut self, key: &str, expected: u64) -> Result<(), StoreError> {
        let old = self.get_record(key)?.ok_or(StoreError::Conflict)?;
        if old.version != expected {
            return Err(StoreError::Conflict);
        }
        for index in Self::index_keys(&old) {
            self.delete_row(&index)?;
        }
        self.delete_row(&format!("r:{key}"))
    }
    fn range(&self, start: &str, end: &str, limit: usize) -> Result<Vec<StoredRecord>, StoreError> {
        let mut request: pb::GetRangeRequest =
            GetRangeRequest::new(&self.store.transport.config.table)
                .start_primary_key_column_string("namespace", &self.store.namespace)
                .start_primary_key_column_string("record_key", start)
                .end_primary_key_column_string("namespace", &self.store.namespace)
                .end_primary_key_column_string("record_key", end)
                .transaction_id(&self.id)
                .limit(limit as i32)
                .into();
        let mut records = Vec::new();
        loop {
            request.limit = Some((limit - records.len()) as i32);
            let response: pb::GetRangeResponse =
                self.store
                    .transport
                    .request("GetRange", &request, self.deadline)?;
            let next = response.next_start_primary_key.clone();
            let decoded: aliyun_tablestore_rs::data::GetRangeResponse =
                response.try_into().map_err(|_| protocol_error())?;
            records.extend(
                decoded
                    .rows
                    .into_iter()
                    .map(decode_record)
                    .collect::<Result<Vec<_>, StoreError>>()?,
            );
            if records.len() >= limit || next.is_none() {
                return Ok(records);
            }
            let next = next.ok_or_else(protocol_error)?;
            if next == request.inclusive_start_primary_key {
                return Err(protocol_error());
            }
            request.inclusive_start_primary_key = next;
        }
    }
    pub fn list<T: DeserializeOwned>(
        &self,
        kind: &str,
        account: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, Versioned<T>)>, StoreError> {
        let kind = URL_SAFE_NO_PAD.encode(kind);
        let prefix = if let Some(account) = account {
            format!("i:a:{}:{kind}:", URL_SAFE_NO_PAD.encode(account))
        } else {
            format!("i:g:{kind}:")
        };
        let start = after
            .map(|a| format!("{prefix}{a}\0"))
            .unwrap_or_else(|| prefix.clone());
        let end = format!("{};", prefix.trim_end_matches(':'));
        self.range(&start, &end, limit)?
            .into_iter()
            .map(|r| {
                Ok((
                    r.key,
                    Versioned {
                        row_version: r.version,
                        value: serde_json::from_slice(&r.payload)
                            .map_err(|_| StoreError::Corrupt)?,
                    },
                ))
            })
            .collect()
    }
    pub fn expired(&self, now: u64, limit: usize) -> Result<Vec<(String, u64)>, StoreError> {
        let end = format!("i:e:{:020};", now);
        Ok(self
            .range("i:e:", &end, limit)?
            .into_iter()
            .filter(|r| r.expires.is_some_and(|e| e <= now))
            .map(|r| (r.key, r.version))
            .collect())
    }
    pub fn commit(&mut self) -> Result<(), StoreError> {
        self.finished = true;
        let _: pb::CommitTransactionResponse = self
            .store
            .transport
            .request(
                "CommitTransaction",
                &pb::CommitTransactionRequest {
                    transaction_id: self.id.clone(),
                },
                self.deadline,
            )
            .map_err(|_| StoreError::OutcomeUnknown)?;
        Ok(())
    }
}
impl Drop for RemoteTransaction {
    fn drop(&mut self) {
        if !self.finished {
            let _: Result<pb::AbortTransactionResponse, _> = self.store.transport.request(
                "AbortTransaction",
                &pb::AbortTransactionRequest {
                    transaction_id: self.id.clone(),
                },
                Instant::now() + Duration::from_secs(5),
            );
        }
    }
}
fn decode_record(row: Row) -> Result<StoredRecord, StoreError> {
    let payload = match row.get_column_value("record") {
        Some(ColumnValue::Blob(b)) => b,
        _ => return Err(StoreError::Corrupt),
    };
    let record: StoredRecord = serde_json::from_slice(payload).map_err(|_| StoreError::Corrupt)?;
    sql_integer(record.version)?;
    if let Some(expires) = record.expires {
        sql_integer(expires)?;
    }
    Ok(record)
}

#[cfg(test)]
#[path = "tablestore_tests.rs"]
mod tests;

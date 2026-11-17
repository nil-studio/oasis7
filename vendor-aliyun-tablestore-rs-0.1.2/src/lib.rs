use std::{collections::HashMap, fmt::Display, str::FromStr, time::Duration};

use base64::{prelude::BASE64_STANDARD, Engine};
use bytes::Bytes;
use defined_column::{AddDefinedColumnOperation, AddDefinedColumnRequest, DeleteDefinedColumnOperation, DeleteDefinedColumnRequest};
use error::OtsError;
use index::{CreateIndexOperation, CreateIndexRequest, DropIndexOperation};
use lastpoint_index::{CreateTimeseriesLastpointIndexOperation, CreateTimeseriesLastpointIndexRequest, DeleteTimeseriesLastpointIndexOperation};
use prost::Message;
use protos::search::{CreateSearchIndexRequest, UpdateSearchIndexRequest};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Response,
};

use analytical_store::{
    CreateTimeseriesAnalyticalStoreOperation, CreateTimeseriesAnalyticalStoreRequest, DeleteTimeseriesAnalyticalStoreOperation,
    DeleteTimeseriesAnalyticalStoreRequest, DescribeTimeseriesAnalyticalStoreOperation, UpdateTimeseriesAnalyticalStoreOperation,
    UpdateTimeseriesAnalyticalStoreRequest,
};
use data::{
    BatchGetRowOperation, BatchGetRowRequest, BatchWriteRowOperation, BatchWriteRowRequest, BulkExportOperation, BulkExportRequest, BulkImportOperation,
    BulkImportRequest, DeleteRowOperation, DeleteRowRequest, GetRangeOperation, GetRangeRequest, GetRowOperation, GetRowRequest, PutRowOperation,
    PutRowRequest, UpdateRowOperation, UpdateRowRequest,
};
use search::{
    ComputeSplitsOperation, CreateSearchIndexOperation, DeleteSearchIndexOperation, DescribeSearchIndexOperation, ListSearchIndexOperation,
    ParallelScanOperation, ParallelScanRequest, SearchOperation, SearchRequest, UpdateSearchIndexOperation,
};
use sql::{SqlQueryOperation, SqlQueryRequest};
use table::{
    ComputeSplitPointsBySizeOperation, ComputeSplitPointsBySizeRequest, CreateTableOperation, CreateTableRequest, DeleteTableOperation, DescribeTableOperation,
    ListTableOperation, UpdateTableOperation, UpdateTableRequest,
};
use timeseries_data::{
    DeleteTimeseriesMetaOperation, DeleteTimeseriesMetaRequest, GetTimeseriesDataOperation, GetTimeseriesDataRequest, PutTimeseriesDataOperation,
    PutTimeseriesDataRequest, QueryTimeseriesMetaOperation, QueryTimeseriesMetaRequest, ScanTimeseriesDataOperation, ScanTimeseriesDataRequest,
    SplitTimeseriesScanTaskOperation, SplitTimeseriesScanTaskRequest, UpdateTimeseriesMetaOperation, UpdateTimeseriesMetaRequest,
};
use timeseries_table::{
    CreateTimeseriesTableOperation, CreateTimeseriesTableRequest, DeleteTimeseriesTableOperation, DescribeTimeseriesTableOperation,
    ListTimeseriesTableOperation, UpdateTimeseriesTableOperation, UpdateTimeseriesTableRequest,
};
use url::Url;
use util::{get_iso8601_date_time_string, hmac_sha256};

pub mod analytical_store;
pub mod crc8;
pub mod data;
pub mod defined_column;
pub mod error;
pub mod index;
pub mod lastpoint_index;
pub mod macros;
pub mod model;
pub mod protos;
pub mod search;
pub mod sql;
pub mod table;
pub mod timeseries_data;
pub mod timeseries_model;
pub mod timeseries_table;
pub mod util;

#[cfg(test)]
pub mod test_util;

const USER_AGENT: &str = "aliyun-tablestore-rs/0.1.2";
const HEADER_API_VERSION: &str = "x-ots-apiversion";
const HEADER_ACCESS_KEY_ID: &str = "x-ots-accesskeyid";
const HEADER_CONTENT_MD5: &str = "x-ots-contentmd5";
const HEADER_SIGNATURE: &str = "x-ots-signature";
const HEADER_DATE: &str = "x-ots-date";
const HEADER_STS_TOKEN: &str = "x-ots-ststoken";
const HEADER_SIGN_REGION: &str = "x-ots-signregion";
const HEADER_SIGN_DATE: &str = "x-ots-signdate";
const HEADER_INSTANCE_NAME: &str = "x-ots-instancename";
const HEADER_SIGNATURE_V4: &str = "x-ots-signaturev4";

const API_VERSION: &str = "2015-12-31";

pub type OtsResult<T> = Result<T, OtsError>;

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtsOp {
    #[default]
    Undefined,

    // tables
    CreateTable,
    UpdateTable,
    ListTable,
    DescribeTable,
    DeleteTable,
    ComputeSplitPointsBySize,

    // defined columns
    AddDefinedColumn,
    DeleteDefinedColumn,

    // Data operations
    GetRow,
    GetRange,
    PutRow,
    UpdateRow,
    DeleteRow,
    BatchGetRow,
    BatchWriteRow,
    BulkImport,
    BulkExport,

    // stream operations
    ListStream,
    DescribeStream,
    GetShardIterator,
    GetStreamRecord,

    // index operations
    CreateIndex,
    DropIndex,

    // timeseries table operations.
    CreateTimeseriesTable,
    ListTimeseriesTable,
    DescribeTimeseriesTable,
    UpdateTimeseriesTable,
    DeleteTimeseriesTable,

    // timeseries table data operations
    PutTimeseriesData,
    GetTimeseriesData,
    UpdateTimeseriesMeta,
    QueryTimeseriesMeta,
    DeleteTimeseriesMeta,
    SplitTimeseriesScanTask,
    ScanTimeseriesData,

    // timeseries lastpoint index
    CreateTimeseriesLastpointIndex,
    DeleteTimeseriesLastpointIndex,

    // timeseries table analyzing operations
    CreateTimeseriesAnalyticalStore,
    UpdateTimeseriesAnalyticalStore,
    DescribeTimeseriesAnalyticalStore,
    DeleteTimeseriesAnalyticalStore,

    // search index operations
    CreateSearchIndex,
    UpdateSearchIndex,
    ListSearchIndex,
    DescribeSearchIndex,
    DeleteSearchIndex,
    Search,
    ComputeSplits,
    ParallelScan,

    // tunnel operations
    CreateTunnel,
    ListTunnel,
    DescribeTunnel,
    DeleteTunnel,

    SQLQuery,
}

impl From<OtsOp> for String {
    fn from(value: OtsOp) -> Self {
        value.to_string()
    }
}

impl Display for OtsOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            OtsOp::Undefined => "_Undefined_",

            OtsOp::CreateTable => "CreateTable",
            OtsOp::UpdateTable => "UpdateTable",
            OtsOp::ListTable => "ListTable",
            OtsOp::DescribeTable => "DescribeTable",
            OtsOp::DeleteTable => "DeleteTable",
            OtsOp::ComputeSplitPointsBySize => "ComputeSplitPointsBySize",

            OtsOp::AddDefinedColumn => "AddDefinedColumn",
            OtsOp::DeleteDefinedColumn => "DeleteDefinedColumn",

            OtsOp::GetRow => "GetRow",
            OtsOp::GetRange => "GetRange",
            OtsOp::PutRow => "PutRow",
            OtsOp::UpdateRow => "UpdateRow",
            OtsOp::DeleteRow => "DeleteRow",
            OtsOp::BatchGetRow => "BatchGetRow",
            OtsOp::BatchWriteRow => "BatchWriteRow",
            OtsOp::BulkImport => "BulkImport",
            OtsOp::BulkExport => "BulkExport",

            OtsOp::CreateTunnel => "CreateTunnel",
            OtsOp::ListTunnel => "ListTunnel",
            OtsOp::DescribeTunnel => "DescribeTunnel",
            OtsOp::DeleteTunnel => "DeleteTunnel",

            OtsOp::ListStream => "ListStream",
            OtsOp::DescribeStream => "DescribeStream",
            OtsOp::GetShardIterator => "GetShardIterator",
            OtsOp::GetStreamRecord => "GetStreamRecord",

            OtsOp::CreateIndex => "CreateIndex",
            OtsOp::DropIndex => "DropIndex",

            OtsOp::CreateTimeseriesTable => "CreateTimeseriesTable",
            OtsOp::ListTimeseriesTable => "ListTimeseriesTable",
            OtsOp::DescribeTimeseriesTable => "DescribeTimeseriesTable",
            OtsOp::UpdateTimeseriesTable => "UpdateTimeseriesTable",
            OtsOp::DeleteTimeseriesTable => "DeleteTimeseriesTable",

            OtsOp::PutTimeseriesData => "PutTimeseriesData",
            OtsOp::GetTimeseriesData => "GetTimeseriesData",
            OtsOp::UpdateTimeseriesMeta => "UpdateTimeseriesMeta",
            OtsOp::QueryTimeseriesMeta => "QueryTimeseriesMeta",
            OtsOp::DeleteTimeseriesMeta => "DeleteTimeseriesMeta",
            OtsOp::SplitTimeseriesScanTask => "SplitTimeseriesScanTask",
            OtsOp::ScanTimeseriesData => "ScanTimeseriesData",

            OtsOp::CreateTimeseriesLastpointIndex => "CreateTimeseriesLastpointIndex",
            OtsOp::DeleteTimeseriesLastpointIndex => "DeleteTimeseriesLastpointIndex",

            OtsOp::CreateTimeseriesAnalyticalStore => "CreateTimeseriesAnalyticalStore",
            OtsOp::UpdateTimeseriesAnalyticalStore => "UpdateTimeseriesAnalyticalStore",
            OtsOp::DescribeTimeseriesAnalyticalStore => "DescribeTimeseriesAnalyticalStore",
            OtsOp::DeleteTimeseriesAnalyticalStore => "DeleteTimeseriesAnalyticalStore",

            OtsOp::CreateSearchIndex => "CreateSearchIndex",
            OtsOp::UpdateSearchIndex => "UpdateSearchIndex",
            OtsOp::ListSearchIndex => "ListSearchIndex",
            OtsOp::DescribeSearchIndex => "DescribeSearchIndex",
            OtsOp::DeleteSearchIndex => "DeleteSearchIndex",
            OtsOp::Search => "Search",
            OtsOp::ComputeSplits => "ComputeSplits",
            OtsOp::ParallelScan => "ParallelScan",

            OtsOp::SQLQuery => "SQLQuery",
        };

        write!(f, "{}", s)
    }
}

impl OtsOp {
    /// 检测一个操作是否是幂等的
    pub fn is_idempotent(&self) -> bool {
        matches!(
            self,
            Self::ListTable
                | Self::DescribeTable
                | Self::GetRow
                | Self::GetRange
                | Self::BatchGetRow
                | Self::BulkExport
                | Self::ListStream
                | Self::DescribeStream
                | Self::GetShardIterator
                | Self::ComputeSplitPointsBySize
                | Self::GetTimeseriesData
                | Self::QueryTimeseriesMeta
                | Self::ListTimeseriesTable
                | Self::DescribeTimeseriesTable
                | Self::ScanTimeseriesData
                | Self::DescribeTimeseriesAnalyticalStore
                | Self::ParallelScan
                | Self::ComputeSplits
                | Self::ListTunnel
                | Self::DescribeTunnel
                | Self::SQLQuery
        )
    }
}

#[derive(Debug, Default, Clone)]
pub struct OtsRequestOptions {
    pub timeout_ms: Option<u64>,
}

/// OTS API 请求结构体
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OtsRequest {
    method: reqwest::Method,
    operation: OtsOp,
    headers: HashMap<String, String>,
    query: HashMap<String, String>,
    body: Vec<u8>,
    options: OtsRequestOptions,
}

impl Default for OtsRequest {
    fn default() -> Self {
        Self {
            method: reqwest::Method::POST,
            operation: OtsOp::Undefined,
            headers: HashMap::new(),
            query: HashMap::new(),
            body: Vec::new(),
            options: OtsRequestOptions::default(),
        }
    }
}

/// 重试策略。需要注意的是 `clone_box` 方法中用来重置策略的状态数据（如果有的话）
pub trait RetryPolicy: std::fmt::Debug + Send + Sync {
    /// 是否需要重试。参数分别表示重试次数、操作和发生的错误
    fn should_retry(&self, retried: u32, op: OtsOp, ots_error: &OtsError) -> bool;

    /// 如果需要重试，重试之前让线程等待的时间
    fn delay_ms(&self) -> u32;

    /// 需要自行实现克隆逻辑。一般来说就是需要重置一些记录参数，为下一次全新的请求做准备
    fn clone_box(&self) -> Box<dyn RetryPolicy>;
}

impl Clone for Box<dyn RetryPolicy> {
    fn clone(&self) -> Box<dyn RetryPolicy> {
        self.clone_box()
    }
}

/// 默认重试机制，做多重试 10 次（加上最开始的 1 次，总计就是发送 11 次请求）。
/// 两次重试之间休眠 10 秒
#[derive(Debug, Copy, Clone)]
pub struct DefaultRetryPolicy {
    pub max_retry_times: u32,
}

impl Default for DefaultRetryPolicy {
    fn default() -> Self {
        Self { max_retry_times: 10 }
    }
}

impl DefaultRetryPolicy {
    /// 无论是什么操作，只要是这些错误码，就重试
    const RETRY_NO_MATTER_ACTIONS_ERR_CODES: &[&'static str] = &[
        "OTSRowOperationConflict",
        "OTSNotEnoughCapacityUnit",
        "OTSTableNotReady",
        "OTSPartitionUnavailable",
        "OTSServerBusy",
    ];

    const ERR_OTS_QUOTA_EXHAUSTED_MSG: &str = "Too frequent table operations.";

    // 仅针对幂等的操作，如果遇到这些错误码，重试
    const RETRY_FOR_IDEMPOTENT_ACTIONS_ERR_CODES: &[&'static str] =
        &["OTSTimeout", "OTSInternalServerError", "OTSServerUnavailable", "OTSTunnelServerUnavailable"];

    fn should_retry_inner(&self, retried: u32, op: OtsOp, ots_error: &OtsError) -> bool {
        if retried >= self.max_retry_times {
            log::info!("max retry reached {} times for operation {} with error {}", self.max_retry_times, op, ots_error);
            return false;
        }

        match ots_error {
            // 网络请求错误，重试
            OtsError::ReqwestError(_) => true,

            // 5xx 的状态码 + 幂等操作，重试
            OtsError::StatusError(code, _) => code.is_server_error() && op.is_idempotent(),

            // API 错误， OTSQuotaExhausted 错误码 + 固定的错误消息，重试
            OtsError::ApiError(api_error)
                if api_error.code == "OTSQuotaExhausted" && api_error.message == Some(Self::ERR_OTS_QUOTA_EXHAUSTED_MSG.to_string()) =>
            {
                true
            }

            // 其他的就是无论什么操作都重试的错误，以及幂等操作对应的错误码
            OtsError::ApiError(api_error) => {
                (Self::RETRY_NO_MATTER_ACTIONS_ERR_CODES.contains(&api_error.code.as_str()))
                    || (op.is_idempotent() && Self::RETRY_FOR_IDEMPOTENT_ACTIONS_ERR_CODES.contains(&api_error.code.as_str()))
            }

            _ => false,
        }
    }
}

impl RetryPolicy for DefaultRetryPolicy {
    fn should_retry(&self, retried: u32, op: OtsOp, ots_error: &OtsError) -> bool {
        self.should_retry_inner(retried, op, ots_error)
    }

    fn clone_box(&self) -> Box<dyn RetryPolicy> {
        Box::new(DefaultRetryPolicy::default())
    }

    fn delay_ms(&self) -> u32 {
        10000
    }
}

/// 客户端构建器
///
/// # Examples
///
/// ```
/// let builder = OtsClient.builder("aid", "asec").endpoint("").instance_name("");
/// let client = builder.build();
/// ```
#[derive(Clone)]
pub struct OtsClientBuilder {
    access_key_id: String,
    access_key_secret: String,
    sts_token: Option<String>,
    retry_policy: Box<dyn RetryPolicy>,
    region: String,
    instance_name: String,
    endpoint: String,
    http_client: Option<reqwest::Client>,
}

impl OtsClientBuilder {
    pub fn new(ak_id: impl AsRef<str>, ak_sec: impl AsRef<str>) -> Self {
        Self {
            access_key_id: ak_id.as_ref().to_string(),
            access_key_secret: ak_sec.as_ref().to_string(),
            retry_policy: Box::new(DefaultRetryPolicy::default()),
            sts_token: None,
            region: String::new(),
            instance_name: String::new(),
            endpoint: String::new(),
            http_client: None,
        }
    }

    /// 设置 STS Token
    pub fn sts_token(mut self, token: impl AsRef<str>) -> Self {
        self.sts_token = Some(token.as_ref().to_string());

        self
    }

    /// 设置重试策略
    pub fn rety_policy(mut self, policy: Box<dyn RetryPolicy>) -> Self {
        self.retry_policy = policy;

        self
    }

    /// 设置地域
    pub fn region(mut self, region: impl AsRef<str>) -> Self {
        self.region = region.as_ref().to_string();

        self
    }

    /// 设置实例名称
    pub fn instance_name(mut self, instance_name: impl AsRef<str>) -> Self {
        self.instance_name = instance_name.as_ref().to_string();

        self
    }

    /// 设置 endpoint。例如：`https://instance-name.cn-beijing.ots.aliyuncs.com`
    pub fn endpoint(mut self, endpoint: impl AsRef<str>) -> Self {
        self.endpoint = endpoint.as_ref().to_string();

        self
    }

    /// 设置自定义的 Client
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http_client = Some(client);

        self
    }

    /// 获取重试策略的可写引用
    pub fn retry_policy_mut(&mut self) -> &mut Box<dyn RetryPolicy> {
        &mut self.retry_policy
    }

    pub fn build(self) -> OtsClient {
        let Self {
            access_key_id,
            access_key_secret,
            sts_token,
            retry_policy,
            region,
            instance_name,
            endpoint,
            http_client,
        } = self;

        OtsClient {
            access_key_id,
            access_key_secret,
            sts_token,
            region,
            instance_name,
            endpoint,
            http_client: http_client.unwrap_or(reqwest::Client::new()),
            retry_policy,
        }
    }
}

/// 客户端
#[allow(dead_code)]
#[derive(Clone)]
pub struct OtsClient {
    access_key_id: String,
    access_key_secret: String,
    sts_token: Option<String>,
    region: String,
    instance_name: String,
    endpoint: String,
    http_client: reqwest::Client,
    retry_policy: Box<dyn RetryPolicy>,
}

impl std::fmt::Debug for OtsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OtsClient")
            .field("access_key_id", &self.access_key_id)
            .field("region", &self.region)
            .field("instance_name", &self.instance_name)
            .field("endpoint", &self.endpoint)
            .field("http_client", &self.http_client)
            .finish()
    }
}

impl OtsClient {
    fn parse_instance_and_region(endpoint: &str) -> (&str, &str) {
        let s = endpoint.strip_prefix("http://").unwrap_or(endpoint);
        let s = s.strip_prefix("https://").unwrap_or(s);
        let parts = s.split(".").collect::<Vec<_>>();
        if parts.len() < 2 {
            panic!("can not parse instance name and region from endpoint: {}", endpoint);
        }

        (parts[0], parts[1])
    }

    /// Build an OtsClient from env values. The following env vars are required:
    ///
    /// - `ALIYUN_OTS_AK_ID`: The access key id.
    /// - `ALIYUN_OTS_AK_SEC`: The access key secret
    /// - `ALIYUN_OTS_ENDPOINT`: The tablestore instance endpoint. e.g. `https://${instance-name}.cn-beijing.ots.aliyuncs.com`
    pub fn from_env() -> Self {
        let access_key_id = std::env::var("ALIYUN_OTS_AK_ID").expect("env var ALI_ACCESS_KEY_ID is missing");
        let access_key_secret = std::env::var("ALIYUN_OTS_AK_SEC").expect("env var ALI_ACCESS_KEY_SECRET is missing");
        let endpoint = std::env::var("ALIYUN_OTS_ENDPOINT").expect("env var ALI_OSS_ENDPOINT is missing");
        let endpoint = endpoint.to_lowercase();
        let (instance_name, region) = Self::parse_instance_and_region(endpoint.as_str());

        Self {
            access_key_id,
            access_key_secret,
            sts_token: None,
            region: region.to_string(),
            instance_name: instance_name.to_string(),
            endpoint,
            http_client: reqwest::Client::new(),
            retry_policy: Box::new(DefaultRetryPolicy::default()),
        }
    }

    /// 使用 AK_ID、AK_SEC 和网络访问地址构建实例
    ///
    /// # Arguments
    ///
    /// - `ak_id`: Access key id
    /// - `ak_sec`: Access key secret
    /// - `endpoint`: 服务地址
    pub fn new(ak_id: impl AsRef<str>, ak_sec: impl AsRef<str>, endpoint: impl AsRef<str>) -> Self {
        let endpoint = endpoint.as_ref().to_lowercase();

        let (instance_name, region) = Self::parse_instance_and_region(endpoint.as_str());

        Self {
            access_key_id: ak_id.as_ref().to_string(),
            access_key_secret: ak_sec.as_ref().to_string(),
            region: region.to_string(),
            instance_name: instance_name.to_string(),
            endpoint,
            http_client: reqwest::Client::new(),
            sts_token: None,
            retry_policy: Box::new(DefaultRetryPolicy::default()),
        }
    }

    /// 客户端构建器
    ///
    /// # Arguments
    ///
    /// - `ak_id`: Access key id
    /// - `ak_sec`: Access key secret
    pub fn builder(ak_id: impl AsRef<str>, ak_sec: impl AsRef<str>) -> OtsClientBuilder {
        OtsClientBuilder::new(ak_id, ak_sec)
    }

    /// V2 版本签名，直接填充请求头 Map
    fn fill_signature_v2(&self, operation: &str, headers: &mut HashMap<String, String>) {
        let date_time_string = get_iso8601_date_time_string();
        let date = &date_time_string[..10].replace("-", "");

        headers.insert("user-agent".to_string(), USER_AGENT.to_string());
        headers.insert(HEADER_API_VERSION.to_string(), API_VERSION.to_string());
        headers.insert(HEADER_DATE.to_string(), date_time_string.clone());
        headers.insert(HEADER_SIGN_DATE.to_string(), date.to_string());
        headers.insert(HEADER_ACCESS_KEY_ID.to_string(), self.access_key_id.clone());
        headers.insert(HEADER_INSTANCE_NAME.to_string(), self.instance_name.clone());
        headers.insert(HEADER_SIGN_REGION.to_string(), self.region.clone());

        if let Some(s) = &self.sts_token {
            headers.insert(HEADER_STS_TOKEN.to_string(), s.to_string());
        }

        let mut canonical_headers = headers
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v))
            .filter(|(k, _)| k.starts_with("x-ots-") && k != HEADER_SIGNATURE)
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>();
        canonical_headers.sort();

        let canonical_headers = canonical_headers.join("\n");

        let string_to_sign = format!("/{}\nPOST\n\n{}\n", operation, canonical_headers);

        log::debug!("string to sign: \n-----\n{}\n-----", string_to_sign);
        let sig = util::hmac_sha1(self.access_key_secret.as_bytes(), string_to_sign.as_bytes());
        let sig_string = BASE64_STANDARD.encode(&sig);

        log::debug!("signature = {}", sig_string);

        headers.insert(HEADER_SIGNATURE.to_string(), sig_string);
    }

    /// V4 版本签名，没调通。阿里云的人建议线用 V2 吧
    #[allow(dead_code)]
    fn fill_signature_v4(&self, operation: &str, headers: &mut HashMap<String, String>) {
        let date_time_string = get_iso8601_date_time_string();
        let date_string = &date_time_string[..10].replace("-", "");

        headers.insert("user-agent".to_string(), USER_AGENT.to_string());
        headers.insert(HEADER_API_VERSION.to_string(), API_VERSION.to_string());
        headers.insert(HEADER_DATE.to_string(), date_time_string.clone());
        headers.insert(HEADER_SIGN_DATE.to_string(), date_string.to_string());
        headers.insert(HEADER_ACCESS_KEY_ID.to_string(), self.access_key_id.clone());
        headers.insert(HEADER_INSTANCE_NAME.to_string(), self.instance_name.clone());
        headers.insert(HEADER_SIGN_REGION.to_string(), self.region.clone());

        if let Some(s) = &self.sts_token {
            headers.insert(HEADER_STS_TOKEN.to_string(), s.to_string());
        }

        let mut canonical_headers = headers
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v))
            .filter(|(k, _)| k.starts_with("x-ots-") && k != HEADER_SIGNATURE_V4)
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>();
        canonical_headers.sort();

        let canonical_headers = canonical_headers.join("\n");

        let string_to_sign = format!("/{}\nPOST\n\n{}\nots", operation, canonical_headers);

        log::debug!("string to sign: \n-----\n{}\n----", string_to_sign);

        let sign = hmac_sha256(self.access_key_secret.as_bytes(), string_to_sign.as_bytes());

        headers.insert(HEADER_SIGNATURE_V4.to_string(), BASE64_STANDARD.encode(sign));
    }

    /// 发送请求
    pub async fn send(&self, req: OtsRequest) -> OtsResult<Response> {
        let OtsRequest {
            method,
            operation,
            mut headers,
            query: _,
            body,
            options,
        } = req;

        // 不会发生变化的请求头
        headers.insert("content-lenght".to_string(), format!("{}", body.len()));
        let content_md5_base64 = BASE64_STANDARD.encode(md5::compute(&body).as_slice());
        headers.insert(HEADER_CONTENT_MD5.to_string(), content_md5_base64);

        let url = Url::parse(format!("{}/{}", self.endpoint, operation).as_str()).unwrap();
        let request_body = Bytes::from_owner(body);
        let mut retried = 0u32;

        loop {
            self.fill_signature_v2(&operation.to_string(), &mut headers);

            let mut header_map = HeaderMap::new();
            headers.iter().for_each(|(k, v)| {
                log::debug!(">> header: {}: {}", k, v);
                header_map.insert(HeaderName::from_str(&k.to_lowercase()).unwrap(), HeaderValue::from_str(v).unwrap());
            });

            let mut request_builder = self
                .http_client
                .request(method.clone(), url.clone())
                .headers(header_map.clone())
                .body(request_body.clone());

            // Handle per-request options
            if let Some(ms) = options.timeout_ms {
                request_builder = request_builder.timeout(Duration::from_millis(ms));
            }

            let response = request_builder.send().await?;

            response.headers().iter().for_each(|(k, v)| {
                log::debug!("<< header: {}: {}", k, v.to_str().unwrap());
            });

            if response.status().is_success() {
                return Ok(response);
            }

            if !&response.status().is_success() {
                let status = response.status();

                let e = match response.bytes().await {
                    Ok(bytes) => {
                        let api_error = protos::Error::decode(bytes)?;
                        OtsError::ApiError(Box::new(api_error))
                    }
                    Err(_) => OtsError::StatusError(status, "".to_string()),
                };

                log::error!("api call failed, check retry against retry policy for operation {} and error {}", operation, e);
                let should_retry = self.retry_policy.should_retry(retried, operation, &e);
                log::info!("should retry: {} for operation {} with error {}", should_retry, operation, e);

                if !should_retry {
                    return Err(e);
                }

                let next_delay = self.retry_policy.delay_ms();
                log::info!("delay for {} ms to retry", next_delay);
                tokio::time::sleep(tokio::time::Duration::from_millis(next_delay as u64)).await;

                retried += 1;
            }
        }
    }

    /// 列出实例下的宽表
    pub fn list_table(&self) -> ListTableOperation {
        ListTableOperation::new(self.clone())
    }

    /// 创建一个宽表
    ///
    /// # Examples
    ///
    /// ```
    /// let req = CreateTableRequest::new("users")
    ///     .primary_key_string("user_id_part")
    ///     .primary_key_string("user_id")
    ///     .column_string("full_name")
    ///     .column_string("phone_number")
    ///     .column_string("pwd_hash")
    ///     .column_string("badge_no")
    ///     .column_string("gender")
    ///     .column_integer("registered_at_ms")
    ///     .column_bool("deleted")
    ///     .column_integer("deleted_at_ms")
    ///     .column_double("score")
    ///     .column_blob("avatar")
    ///     .index(
    ///         IndexMetaBuilder::new("idx_phone_no")
    ///             .primary_key("user_id_part")
    ///             .defined_column("phone_number")
    ///             .index_type(IndexType::ItGlobalIndex)
    ///             .build(),
    ///     );
    /// let response = client.create_table(req).send().await;
    /// ```
    pub fn create_table(&self, request: CreateTableRequest) -> CreateTableOperation {
        CreateTableOperation::new(self.clone(), request)
    }

    /// 更新宽表定义
    pub fn update_table(&self, request: UpdateTableRequest) -> UpdateTableOperation {
        UpdateTableOperation::new(self.clone(), request)
    }

    /// 获取宽表定义
    pub fn describe_table(&self, table_name: &str) -> DescribeTableOperation {
        DescribeTableOperation::new(self.clone(), table_name)
    }

    /// 删除宽表
    pub fn delete_table(&self, table_name: &str) -> DeleteTableOperation {
        DeleteTableOperation::new(self.clone(), table_name)
    }

    /// 计算宽表分裂点
    pub fn compute_split_points_by_size(&self, request: ComputeSplitPointsBySizeRequest) -> ComputeSplitPointsBySizeOperation {
        ComputeSplitPointsBySizeOperation::new(self.clone(), request)
    }

    /// 添加预定义列
    ///
    /// # Examples
    ///
    /// ```
    /// let response = client
    ///     .add_defined_column(
    ///         AddDefinedColumnRequest::new("ccs")
    ///             .column_integer("created_at")
    ///             .column_string("cover_url")
    ///             .column_double("avg_score"),
    ///     )
    ///     .send()
    ///     .await;
    /// ```
    pub fn add_defined_column(&self, request: AddDefinedColumnRequest) -> AddDefinedColumnOperation {
        AddDefinedColumnOperation::new(self.clone(), request)
    }

    /// 删除预定义列
    ///
    /// # Example
    ///
    /// ```
    /// let response = client
    ///     .delete_defined_column(DeleteDefinedColumnRequest::new("ccs").column("created_at"))
    ///     .send()
    ///     .await;
    /// ```
    pub fn delete_defined_column(&self, request: DeleteDefinedColumnRequest) -> DeleteDefinedColumnOperation {
        DeleteDefinedColumnOperation::new(self.clone(), request)
    }

    /// 根据主键获取单行数据
    ///
    /// # Examples
    ///
    /// ```
    /// let response = client
    ///     .get_row(
    ///         GetRowRequest::new("schools")
    ///             .primary_key_string("school_id", "00020FFB-BB14-CCAD-0181-A929E71C7312")
    ///             .primary_key_integer("id", 1742203524276000)
    ///             .max_versions(1),
    ///     )
    ///     .send()
    ///     .await;
    /// ```
    pub fn get_row(&self, request: GetRowRequest) -> GetRowOperation {
        GetRowOperation::new(self.clone(), request)
    }

    /// 根据主键获取范围数据
    ///
    /// # Examples
    ///
    /// ## 依次设置开始主键和结束主键
    ///
    /// ```
    /// let response = client.get_range(
    ///     GetRangeRequest::new("table_name")
    ///         .start_primary_key_string("id", "string_id_value")
    ///         .start_primary_key_inf_min("long_id")
    ///         .end_primary_key_string("id", "string_id_value")
    ///         .end_primary_key_inf_max("long_id")
    ///         .direction(Direction::Forward)
    /// ).send().await;
    /// ```
    ///
    /// ## 依次设置每个主键的开始和结束值
    ///
    /// ```
    /// let response = client.get_range(
    ///     GetRangeRequest::new("table_name").primary_key_range(
    ///         "id",
    ///         PrimaryKeyValue::String("string_id_value".to_string()),
    ///         PrimaryKeyValue::String("string_id_value".to_string())
    ///     ).primary_key_range(
    ///         "long_id",
    ///         PrimaryKeyValue::Integer(12345678),
    ///         PrimaryKeyValue::InfMax
    ///     ).direction(Direction::Forward)
    /// ).send().await;
    /// ```
    pub fn get_range(&self, request: GetRangeRequest) -> GetRangeOperation {
        GetRangeOperation::new(self.clone(), request)
    }

    /// 插入一行数据
    ///
    /// # Examples
    ///
    /// ```
    /// let row = Row::default()
    ///     .primary_key_string("school_id", &school_id)
    ///     .primary_key_auto_increment("id")
    ///     .column_string("name", Name(ZH_CN).fake::<String>())
    ///     .column_string("province", Name(ZH_CN).fake::<String>());
    ///
    /// let response = client
    ///     .put_row(
    ///         PutRowRequest::new("schools").row(row).return_type(ReturnType::RtPk)
    ///     ).send().await.unwrap();
    /// ```
    pub fn put_row(&self, request: PutRowRequest) -> PutRowOperation {
        PutRowOperation::new(self.clone(), request)
    }

    /// 更新一行数据
    ///
    /// # Examples
    ///
    /// ```
    /// let response = client
    ///     .update_row(
    ///         UpdateRowRequest::new(table_name)
    ///             .row(
    ///                 Row::new()
    ///                     .primary_key_string("str_id", &id)
    ///                     .column_string("str_col", "b")
    ///                     .column_to_increse("int_col", 1)
    ///                     .column_bool("bool_col", true)
    ///                     .column_to_delete_all_versions("blob_col"),
    ///             )
    ///             .return_type(ReturnType::RtPk),
    ///     )
    ///     .send()
    ///     .await;
    /// ```
    pub fn update_row(&self, request: UpdateRowRequest) -> UpdateRowOperation {
        UpdateRowOperation::new(self.clone(), request)
    }

    /// 根据主键删除数据行
    ///
    /// # Examples
    ///
    /// ```
    /// client.delete_row(
    ///     DeleteRowRequest::new(table_name).primary_key_string("str_id", &id)
    /// ).send().await;
    /// ```
    pub fn delete_row(&self, request: DeleteRowRequest) -> DeleteRowOperation {
        DeleteRowOperation::new(self.clone(), request)
    }

    /// 批量读取一个表或多个表中的若干行数据
    ///
    /// # Examples
    ///
    /// ```
    /// let client = OtsClient::from_env();
    ///
    /// let t1 = TableInBatchGetRowRequest::new("data_types")
    ///     .primary_key(
    ///         PrimaryKey::new().column_string("str_id", "1")
    ///     ).primary_key(
    ///         PrimaryKey::new().column_string("str_id", "02421870-56d8-4429-a548-27e0e1f42894")
    ///     );
    ///
    /// let t2 = TableInBatchGetRowRequest::new("schools").primary_key(
    ///     PrimaryKey::new().column_string("school_id", "00020FFB-BB14-CCAD-0181-A929E71C7312")
    ///         .column_integer("id", 1742203524276000)
    /// );
    ///
    /// let request = BatchGetRowRequest::new().tables(
    ///     vec![t1, t2]
    /// );
    ///
    /// let res = client.batch_get_row(request).send().await;
    /// ```
    pub fn batch_get_row(&self, request: BatchGetRowRequest) -> BatchGetRowOperation {
        BatchGetRowOperation::new(self.clone(), request)
    }

    /// 接口批量插入、修改或删除一个或多个表中的若干行数据。
    ///
    /// # Examples
    ///
    /// ```
    /// let client = OtsClient::from_env();
    ///
    /// let uuid: String = UUIDv4.fake();
    ///
    /// let t1 = TableInBatchWriteRowRequest::new("data_types").rows(vec![
    ///     RowInBatchWriteRowRequest::put_row(
    ///         Row::new()
    ///             .primary_key_column_string("str_id", &uuid)
    ///             .column_string("str_col", "column is generated from batch writing"),
    ///     ),
    ///     RowInBatchWriteRowRequest::delete_row(Row::new().primary_key_column_string("str_id", "266e79aa-eb74-47d8-9658-e17d52fc012d")),
    ///     RowInBatchWriteRowRequest::update_row(
    ///         Row::new()
    ///             .primary_key_column_string("str_id", "975e9e17-f969-4387-9cef-a6ae9849a10d")
    ///             .column_double("double_col", 11.234),
    ///     ),
    /// ]);
    ///
    /// let t2 = TableInBatchWriteRowRequest::new("schools").rows(vec![RowInBatchWriteRowRequest::update_row(
    ///     Row::new()
    ///         .primary_key_column_string("school_id", "2")
    ///         .primary_key_column_integer("id", 1742378007415000)
    ///         .column_string("name", "School-AAAA"),
    /// )]);
    ///
    /// let req = BatchWriteRowRequest::new().table(t1).table(t2);
    ///
    /// let res = client.batch_write_row(req).send().await;
    /// ```
    pub fn batch_write_row(&self, request: BatchWriteRowRequest) -> BatchWriteRowOperation {
        BatchWriteRowOperation::new(self.clone(), request)
    }

    /// 批量写入数据。写入数据时支持插入一行数据、修改行数据以及删除行数据。最多一次 200 行
    ///
    /// # Examples
    ///
    /// ```
    /// let client = OtsClient::from_env();
    /// let mut req = BulkImportRequest::new("data_types");
    /// for i in 0..200 {
    ///     let id: String = UUIDv4.fake();
    ///     let mut blob_val = [0u8; 16];
    ///     rand::fill(&mut blob_val);
    ///     let bool_val = i % 2 == 0;
    ///     let double_val = rand::random_range::<f64, _>(0.0f64..99.99f64);
    ///     let int_val = rand::random_range::<i64, _>(0..10000);
    ///     let str_val: String = Name(ZH_CN).fake();
    ///     let row = Row::new()
    ///         .primary_key_column_string("str_id", &id)
    ///         .column_blob("blob_col", blob_val)
    ///         .column_bool("bool_col", bool_val)
    ///         .column_double("double_col", double_val)
    ///         .column_integer("int_col", int_val)
    ///         .column_string("str_col", &str_val);
    ///     req = req.put_row(row);
    /// }
    /// let res = client.bulk_import(req).send().await;
    /// ```
    pub fn bulk_import(&self, request: BulkImportRequest) -> BulkImportOperation {
        BulkImportOperation::new(self.clone(), request)
    }

    /// 接口批量导出数据。
    ///
    /// # Examples
    ///
    /// ```
    /// let request = BulkExportRequest::new("data_types")
    ///     .end_primary_key_column_inf_min("str_id")
    ///     .end_primary_key_column_inf_max("str_id")
    ///     .columns_to_get(["str_id", "str_col", "int_col", "double_col", "blob_col", "bool_col"]);
    ///
    /// let res = client.bulk_export(request).send().await;
    /// let res = res.unwrap();
    /// total_rows += res.rows.len();
    ///
    /// res.rows.iter().for_each(|r| {
    ///     log::debug!("row: {:?}", r.get_primary_key_value("str_id").unwrap());
    /// });
    /// ```
    pub fn bulk_export(&self, request: BulkExportRequest) -> BulkExportOperation {
        BulkExportOperation::new(self.clone(), request)
    }

    /// 创建二级索引
    pub fn create_index(&self, request: CreateIndexRequest) -> CreateIndexOperation {
        CreateIndexOperation::new(self.clone(), request)
    }

    /// 删除二级索引
    pub fn drop_index(&self, table_name: &str, idx_name: &str) -> DropIndexOperation {
        DropIndexOperation::new(self.clone(), table_name, idx_name)
    }

    /// 列出多元索引
    pub fn list_search_index(&self, table_name: Option<&str>) -> ListSearchIndexOperation {
        ListSearchIndexOperation::new(self.clone(), table_name)
    }

    /// 创建多元索引
    pub fn create_search_index(&self, request: CreateSearchIndexRequest) -> CreateSearchIndexOperation {
        CreateSearchIndexOperation::new(self.clone(), request)
    }

    /// 查询多元索引描述信息
    pub fn describe_search_index(&self, table_name: &str, index_name: &str) -> DescribeSearchIndexOperation {
        DescribeSearchIndexOperation::new(self.clone(), table_name, index_name)
    }

    /// 修改多元索引
    pub fn update_search_index(&self, request: UpdateSearchIndexRequest) -> UpdateSearchIndexOperation {
        UpdateSearchIndexOperation::new(self.clone(), request)
    }

    /// 删除多元索引
    pub fn delete_search_index(&self, table_name: &str, index_name: &str) -> DeleteSearchIndexOperation {
        DeleteSearchIndexOperation::new(self.clone(), table_name, index_name)
    }

    /// 通过多元索引查询数据
    pub fn search(&self, request: SearchRequest) -> SearchOperation {
        SearchOperation::new(self.clone(), request)
    }

    /// 计算多元索引的并发度
    pub fn compute_splits(&self, table_name: &str, index_name: &str) -> ComputeSplitsOperation {
        ComputeSplitsOperation::new(self.clone(), table_name, index_name)
    }

    /// 并行扫描
    pub fn parallel_scan(&self, request: ParallelScanRequest) -> ParallelScanOperation {
        ParallelScanOperation::new(self.clone(), request)
    }

    /// 时序表 - 查询数据
    pub fn get_timeseries_data(&self, request: GetTimeseriesDataRequest) -> GetTimeseriesDataOperation {
        GetTimeseriesDataOperation::new(self.clone(), request)
    }

    /// 时序表 - 写入数据
    ///
    /// # Examples
    ///
    /// ```
    /// let client = OtsClient::from_env();
    ///
    /// let ts_us = (current_time_ms() * 1000) as u64;
    ///
    /// let request = PutTimeseriesDataRequest::new("timeseries_demo_with_data")
    ///     .row(
    ///         TimeseriesRow::new()
    ///             .measurement_name("measure_11")
    ///             .datasource("data_11")
    ///             .tag("cluster", "cluster_11")
    ///             .tag("region", "region_11")
    ///             .timestamp_us(ts_us)
    ///             .field_integer("temp", 123),
    ///     )
    ///     .row(
    ///         TimeseriesRow::new()
    ///             .measurement_name("measure_11")
    ///             .datasource("data_11")
    ///             .tag("cluster", "cluster_11")
    ///             .tag("region", "region_11")
    ///             .timestamp_us(ts_us + 1000)
    ///             .field_double("temp", 543.21),
    ///     );
    ///
    /// let resp = client.put_timeseries_data(request).send().await;
    /// ```
    pub fn put_timeseries_data(&self, request: PutTimeseriesDataRequest) -> PutTimeseriesDataOperation {
        PutTimeseriesDataOperation::new(self.clone(), request)
    }

    /// 时序表 - 创建时序表
    pub fn create_timeseries_table(&self, request: CreateTimeseriesTableRequest) -> CreateTimeseriesTableOperation {
        CreateTimeseriesTableOperation::new(self.clone(), request)
    }

    /// 时序表 - 查询时序表信息
    pub fn describe_timeseries_table(&self, table_name: &str) -> DescribeTimeseriesTableOperation {
        DescribeTimeseriesTableOperation::new(self.clone(), table_name)
    }

    /// 时序表 - 列出时序表
    pub fn list_timeseries_table(&self) -> ListTimeseriesTableOperation {
        ListTimeseriesTableOperation::new(self.clone())
    }

    /// 时序表 - 更新表配置
    pub fn update_timeseries_table(&self, request: UpdateTimeseriesTableRequest) -> UpdateTimeseriesTableOperation {
        UpdateTimeseriesTableOperation::new(self.clone(), request)
    }

    /// 时序表 - 删除时序表
    pub fn delete_timeseries_table(&self, table_name: &str) -> DeleteTimeseriesTableOperation {
        DeleteTimeseriesTableOperation::new(self.clone(), table_name)
    }

    /// 时序表 - 创建 lastpoint 索引
    pub fn create_timeseries_lastpoint_index(&self, request: CreateTimeseriesLastpointIndexRequest) -> CreateTimeseriesLastpointIndexOperation {
        CreateTimeseriesLastpointIndexOperation::new(self.clone(), request)
    }

    /// 时序表 - 删除 lastpoint 索引
    pub fn delete_timeseries_lastpoint_index(&self, table_name: &str, index_name: &str) -> DeleteTimeseriesLastpointIndexOperation {
        DeleteTimeseriesLastpointIndexOperation::new(self.clone(), table_name, index_name)
    }

    /// 时序表 - 创建分析存储
    pub fn create_timeseries_analytical_store(&self, request: CreateTimeseriesAnalyticalStoreRequest) -> CreateTimeseriesAnalyticalStoreOperation {
        CreateTimeseriesAnalyticalStoreOperation::new(self.clone(), request)
    }

    /// 时序表 - 更新分析存储
    pub fn update_timeseries_analytical_store(&self, request: UpdateTimeseriesAnalyticalStoreRequest) -> UpdateTimeseriesAnalyticalStoreOperation {
        UpdateTimeseriesAnalyticalStoreOperation::new(self.clone(), request)
    }

    /// 时序表 - 删除分析存储
    pub fn delete_timeseries_analytical_store(&self, request: DeleteTimeseriesAnalyticalStoreRequest) -> DeleteTimeseriesAnalyticalStoreOperation {
        DeleteTimeseriesAnalyticalStoreOperation::new(self.clone(), request)
    }

    /// 时序表 - 查询分析存储的信息
    pub fn describe_timeseries_analytical_store(&self, table_name: &str, store_name: &str) -> DescribeTimeseriesAnalyticalStoreOperation {
        DescribeTimeseriesAnalyticalStoreOperation::new(self.clone(), table_name, store_name)
    }

    /// 时序表 - 查询元数据
    pub fn query_timeseries_meta(&self, request: QueryTimeseriesMetaRequest) -> QueryTimeseriesMetaOperation {
        QueryTimeseriesMetaOperation::new(self.clone(), request)
    }

    /// 时序表 - 更新时间线元数据
    pub fn update_timeseries_meta(&self, request: UpdateTimeseriesMetaRequest) -> UpdateTimeseriesMetaOperation {
        UpdateTimeseriesMetaOperation::new(self.clone(), request)
    }

    /// 时序表 - 删除时间线元数据
    pub fn delete_timeseries_meta(&self, request: DeleteTimeseriesMetaRequest) -> DeleteTimeseriesMetaOperation {
        DeleteTimeseriesMetaOperation::new(self.clone(), request)
    }

    /// 时序表 - 切分全量导出任务
    pub fn split_timeseries_scan_task(&self, request: SplitTimeseriesScanTaskRequest) -> SplitTimeseriesScanTaskOperation {
        SplitTimeseriesScanTaskOperation::new(self.clone(), request)
    }

    /// 时序表 - 扫描数据
    pub fn scan_timeseries_data(&self, request: ScanTimeseriesDataRequest) -> ScanTimeseriesDataOperation {
        ScanTimeseriesDataOperation::new(self.clone(), request)
    }

    /// SQL 查询。注意：返回的行中，主键值和普通列的值都放到普通列 `columns` 或者 `fields` 中了。
    ///
    /// # Examples
    ///
    /// ```
    /// let client = OtsClient::from_env();
    /// let req = SqlQueryRequest::new("select * from timeseries_demo_with_data where _m_name = 'measure_11'");
    /// let resp = client.sql_query(req).send::<TimeseriesRow>().await;
    /// log::debug!("timeseries table: {:?}", resp)
    /// ```
    pub fn sql_query(&self, request: SqlQueryRequest) -> SqlQueryOperation {
        SqlQueryOperation::new(self.clone(), request)
    }
}

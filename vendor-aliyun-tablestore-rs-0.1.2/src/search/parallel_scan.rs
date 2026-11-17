use std::collections::HashSet;

use prost::Message;

use super::Query;
use crate::model::rules::{validate_index_name, validate_table_name};
use crate::OtsRequestOptions;
use crate::{
    add_per_request_options,
    error::OtsError,
    model::Row,
    protos::{plain_buffer::MASK_HEADER, search::ColumnReturnType},
    OtsClient, OtsOp, OtsRequest, OtsResult,
};

/// 在ParallelScan操作中表示扫描查询配置
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/scanquery>
#[derive(Debug, Clone)]
pub struct ScanQuery {
    /// 查询条件。支持精确查询、模糊查询、范围查询、地理位置查询、嵌套查询等
    pub query: Query,

    /// 最大并发数。请求支持的最大并发数由用户数据量决定。数据量越大，支持的并发数越多
    pub max_parallel: u32,

    /// 当前并发ID。取值范围为 `[0, max_parallel)`。
    pub current_parallel_id: u32,

    /// 扫描数据时一次能返回的数据行数
    pub limit: Option<u32>,

    /// ParallelScan 的当前任务有效时间，也是 token 的有效时间。默认值为 `60`，建议使用默认值，单位为秒。
    /// 如果在有效时间内没有发起下一次请求，则不能继续读取数据。持续发起请求会刷新 `token` 有效时间。
    pub alive_time_second: Option<u32>,

    /// 用于翻页功能。
    ///
    /// `ParallelScan` 请求结果中有下一次进行翻页的 `token`，使用该 `token` 可以接着上一次的结果继续读取数据
    pub token: Option<Vec<u8>>,
}

impl ScanQuery {
    pub fn new(query: Query, max_parallel: u32, current_parallel_id: u32) -> Self {
        Self {
            query,
            max_parallel,
            current_parallel_id,
            limit: None,
            alive_time_second: None,
            token: None,
        }
    }

    /// 设置一次返回的数据行数
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// 设置当前任务有效时间
    pub fn alive_time_second(mut self, alive_time_second: u32) -> Self {
        self.alive_time_second = Some(alive_time_second);
        self
    }

    /// 设置用于翻页的token
    pub fn token(mut self, token: impl Into<Vec<u8>>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if self.max_parallel == 0 {
            return Err(OtsError::ValidationFailed("max_parallel must be greater than 0".to_string()));
        }

        if self.current_parallel_id >= self.max_parallel {
            return Err(OtsError::ValidationFailed("current_parallel_id must be less than max_parallel".to_string()));
        }

        if let Some(n) = self.limit {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed("limit must be less than i32::MAX".to_string()));
            }
        }

        if let Some(n) = self.alive_time_second {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed("alive_time_second must be less than i32::MAX".to_string()));
            }
        }

        Ok(())
    }
}

impl From<ScanQuery> for crate::protos::search::ScanQuery {
    fn from(value: ScanQuery) -> Self {
        let ScanQuery {
            query,
            max_parallel,
            current_parallel_id,
            limit,
            alive_time_second,
            token,
        } = value;

        Self {
            query: Some(query.into()),
            limit: limit.map(|n| n as i32),
            alive_time: alive_time_second.map(|n| n as i32),
            token,
            current_parallel_id: Some(current_parallel_id as i32),
            max_parallel: Some(max_parallel as i32),
        }
    }
}

/// 并行扫描请求
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/parallelscan>
#[derive(Debug, Clone)]
pub struct ParallelScanRequest {
    /// 数据表名称
    pub table_name: String,

    /// 多元索引名称
    pub index_name: String,

    /// 扫描查询配置
    pub scan_query: ScanQuery,

    /// 当前 sessionId。使用 sessionId 能够保证获取到的结果集是稳定的
    pub session_id: Option<Vec<u8>>,

    /// 返回列类型
    pub column_return_type: Option<ColumnReturnType>,

    /// 需要获取的列
    pub columns_to_get: HashSet<String>,

    /// 请求超时时间，单位为毫秒
    pub timeout_ms: Option<u32>,
}

impl ParallelScanRequest {
    pub fn new(table_name: &str, index_name: &str, scan_query: ScanQuery) -> Self {
        Self {
            table_name: table_name.to_string(),
            index_name: index_name.to_string(),
            scan_query,
            session_id: None,
            columns_to_get: HashSet::new(),
            timeout_ms: None,
            column_return_type: None,
        }
    }

    /// 设置数据表名称
    pub fn table_name(mut self, table_name: String) -> Self {
        self.table_name = table_name;
        self
    }

    /// 设置多元索引名称
    pub fn index_name(mut self, index_name: String) -> Self {
        self.index_name = index_name;
        self
    }

    /// 设置扫描查询配置
    pub fn scan_query(mut self, scan_query: ScanQuery) -> Self {
        self.scan_query = scan_query;
        self
    }

    /// 设置当前 sessionId
    pub fn session_id(mut self, session_id: impl Into<Vec<u8>>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// 设置返回列类型
    pub fn column_return_type(mut self, column_return_type: ColumnReturnType) -> Self {
        self.column_return_type = Some(column_return_type);
        self
    }

    /// 添加一个需要获取的列
    pub fn column_to_get(mut self, column: impl Into<String>) -> Self {
        self.columns_to_get.insert(column.into());
        self
    }

    /// 设置需要获取的列
    pub fn columns_to_get(mut self, columns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.columns_to_get = columns.into_iter().map(|c| c.into()).collect();
        self
    }

    /// 设置请求超时时间
    pub fn timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if !validate_index_name(&self.index_name) {
            return Err(OtsError::ValidationFailed(format!("invalid index name: {}", self.index_name)));
        }

        if let Some(t) = &self.column_return_type {
            if t == &ColumnReturnType::ReturnAll {
                return Err(OtsError::ValidationFailed(
                    "column_return_type RETURN_ALL is not supported. please use RETURN_ALL_FROM_INDEX instead".to_string(),
                ));
            }
        }

        self.scan_query.validate()?;

        Ok(())
    }
}

impl From<ParallelScanRequest> for crate::protos::search::ParallelScanRequest {
    fn from(value: ParallelScanRequest) -> Self {
        let ParallelScanRequest {
            table_name,
            index_name,
            scan_query,
            session_id,
            columns_to_get,
            timeout_ms,
            column_return_type,
        } = value;

        Self {
            table_name: Some(table_name),
            index_name: Some(index_name),
            scan_query: Some(scan_query.into()),
            session_id,
            columns_to_get: Some(crate::protos::search::ColumnsToGet {
                return_type: column_return_type.map(|n| n as i32),
                column_names: columns_to_get.into_iter().collect(),
            }),
            timeout_ms: timeout_ms.map(|n| n as i32),
        }
    }
}

/// 并行扫描响应
#[derive(Debug, Clone)]
pub struct ParallelScanResponse {
    /// 扫描到的数据行
    pub rows: Vec<Row>,

    /// 下一次扫描的 token
    pub next_token: Option<Vec<u8>>,
}

impl TryFrom<crate::protos::search::ParallelScanResponse> for ParallelScanResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::ParallelScanResponse) -> Result<Self, Self::Error> {
        let crate::protos::search::ParallelScanResponse { rows: rows_bytes, next_token } = value;

        let mut rows = vec![];
        for row_bytes in rows_bytes {
            if !row_bytes.is_empty() {
                rows.push(Row::decode_plain_buffer(row_bytes, MASK_HEADER)?);
            }
        }

        Ok(Self { rows, next_token })
    }
}

#[derive(Clone)]
pub struct ParallelScanOperation {
    client: OtsClient,
    request: ParallelScanRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(ParallelScanOperation);

impl ParallelScanOperation {
    pub(crate) fn new(client: OtsClient, request: ParallelScanRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<ParallelScanResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::search::ParallelScanRequest::from(request);
        let req = OtsRequest {
            operation: OtsOp::ParallelScan,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        let resp_msg = crate::protos::search::ParallelScanResponse::decode(resp.bytes().await?)?;

        ParallelScanResponse::try_from(resp_msg)
    }
}

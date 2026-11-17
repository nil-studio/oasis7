use prost::Message;

use crate::model::rules::{validate_index_name, validate_table_name};
use crate::{add_per_request_options, error::OtsError, OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult};

/// 调用 ComputeSplits 接口获取当前 ParallelScan 单个请求的最大并发数，用于使用多元索引并发导出数据时的并发度规划。
#[derive(Debug, Clone, Default)]
pub struct ComputeSplitsRequest {
    /// 表名称
    pub table_name: String,

    /// 多元索引名称
    pub index_name: String,
}

impl ComputeSplitsRequest {
    pub fn new(table_name: &str, index_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            index_name: index_name.to_string(),
        }
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed("table_name is invalid".to_string()));
        }

        if !validate_index_name(&self.index_name) {
            return Err(OtsError::ValidationFailed("index_name is invalid".to_string()));
        }

        Ok(())
    }
}

impl From<ComputeSplitsRequest> for crate::protos::ComputeSplitsRequest {
    fn from(request: ComputeSplitsRequest) -> Self {
        let ComputeSplitsRequest { table_name, index_name } = request;
        Self {
            table_name: Some(table_name),
            search_index_splits_options: Some(crate::protos::SearchIndexSplitsOptions { index_name: Some(index_name) }),
        }
    }
}

/// 获取最大并发数的结果
#[derive(Debug, Clone, Default)]
pub struct ComputeSplitsResponse {
    /// 当前 sessionId。使用 sessionId 能够保证获取到的结果集是稳定的
    pub session_id: Vec<u8>,

    /// ParallelScan任务支持的最大并发数
    pub splits_size: u32,
}

impl TryFrom<crate::protos::ComputeSplitsResponse> for ComputeSplitsResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::ComputeSplitsResponse) -> Result<Self, Self::Error> {
        let crate::protos::ComputeSplitsResponse { session_id, splits_size } = value;

        if session_id.is_none() || splits_size.is_none() {
            return Err(OtsError::ValidationFailed("session_id or splits_size is none".to_string()));
        }

        Ok(Self {
            session_id: session_id.unwrap(),
            splits_size: splits_size.unwrap() as u32,
        })
    }
}
/// 计算最大并发数
#[derive(Clone)]
pub struct ComputeSplitsOperation {
    client: OtsClient,
    request: ComputeSplitsRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(ComputeSplitsOperation);

impl ComputeSplitsOperation {
    pub(crate) fn new(client: OtsClient, table_name: &str, index_name: &str) -> Self {
        Self {
            client,
            request: ComputeSplitsRequest::new(table_name, index_name),
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<ComputeSplitsResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::ComputeSplitsRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::ComputeSplits,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        let resp_msg = crate::protos::ComputeSplitsResponse::decode(resp.bytes().await?)?;

        ComputeSplitsResponse::try_from(resp_msg)
    }
}

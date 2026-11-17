use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    protos::timeseries::DeleteTimeseriesMetaResponse,
    timeseries_model::{rules::validate_timeseries_table_name, TimeseriesKey, SUPPORTED_TABLE_VERSION},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 删除时间线元数据
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/deletetimeseriesmeta>
#[derive(Debug, Default, Clone)]
pub struct DeleteTimeseriesMetaRequest {
    /// 时序表名称
    pub table_name: String,

    /// 要删除的时间线标识
    pub keys: Vec<TimeseriesKey>,
}

impl DeleteTimeseriesMetaRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 添加一个时间线标识
    pub fn key(mut self, key: TimeseriesKey) -> Self {
        self.keys.push(key);

        self
    }

    /// 设置时间线标识
    pub fn keys(mut self, keys: impl IntoIterator<Item = TimeseriesKey>) -> Self {
        self.keys = keys.into_iter().collect();

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid timeseries table name: {}", self.table_name)));
        }

        if self.keys.is_empty() {
            return Err(OtsError::ValidationFailed("keys to delete can not be empty".to_string()));
        }

        for k in &self.keys {
            k.validate()?;
        }

        Ok(())
    }
}

impl From<DeleteTimeseriesMetaRequest> for crate::protos::timeseries::DeleteTimeseriesMetaRequest {
    fn from(value: DeleteTimeseriesMetaRequest) -> Self {
        let DeleteTimeseriesMetaRequest { table_name, keys } = value;

        Self {
            table_name,
            timeseries_key: keys.into_iter().map(crate::protos::timeseries::TimeseriesKey::from).collect(),
            supported_table_version: Some(SUPPORTED_TABLE_VERSION),
        }
    }
}

#[derive(Clone)]
pub struct DeleteTimeseriesMetaOperation {
    client: OtsClient,
    request: DeleteTimeseriesMetaRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(DeleteTimeseriesMetaOperation);

impl DeleteTimeseriesMetaOperation {
    pub(crate) fn new(client: OtsClient, request: DeleteTimeseriesMetaRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<DeleteTimeseriesMetaResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::DeleteTimeseriesMetaRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::DeleteTimeseriesMeta,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        Ok(crate::protos::timeseries::DeleteTimeseriesMetaResponse::decode(resp.bytes().await?)?)
    }
}

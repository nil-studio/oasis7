use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    timeseries_model::rules::{validate_analytical_store_name, validate_timeseries_table_name, MIN_ANALYTICAL_STORE_TTL_SECONDS},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 更新时序分析存储配置信息，目前仅支持修改数据生命周期TTL。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/updatetimeseriesanalyticalstore-api-of-tablestore>
#[derive(Debug, Clone, Default)]
pub struct UpdateTimeseriesAnalyticalStoreRequest {
    /// 时序表名
    pub table_name: String,

    /// 分析存储名
    pub store_name: String,

    /// 分析存储数据保留时间。取值必须大于等于 `2592000` 秒（即 `30` 天）或者必须为 `-1`（数据永不过期）
    pub ttl_seconds: Option<i32>,
}

impl UpdateTimeseriesAnalyticalStoreRequest {
    pub fn new(table_name: &str, store_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            store_name: store_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置分析存储数据保留时间。取值必须大于等于 `2592000` 秒（即 `30` 天）或者必须为 `-1`（数据永不过期）
    pub fn ttl_seconds(mut self, ttl: i32) -> Self {
        self.ttl_seconds = Some(ttl);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid timeseries table name: {}", self.table_name)));
        }

        if !validate_analytical_store_name(&self.store_name) {
            return Err(OtsError::ValidationFailed(format!(
                "invalid timeseries analytical store name: {}",
                self.store_name
            )));
        }

        if let Some(n) = self.ttl_seconds {
            if n != -1 && n < MIN_ANALYTICAL_STORE_TTL_SECONDS {
                return Err(OtsError::ValidationFailed(format!(
                    "invalid store data ttl (seconds): {}. must be -1 or greater than {}",
                    n, MIN_ANALYTICAL_STORE_TTL_SECONDS
                )));
            }
        }

        Ok(())
    }
}

impl From<UpdateTimeseriesAnalyticalStoreRequest> for crate::protos::timeseries::UpdateTimeseriesAnalyticalStoreRequest {
    fn from(value: UpdateTimeseriesAnalyticalStoreRequest) -> Self {
        let UpdateTimeseriesAnalyticalStoreRequest {
            table_name,
            store_name,
            ttl_seconds,
        } = value;

        Self {
            table_name,
            analytical_store: crate::protos::timeseries::TimeseriesAnalyticalStore {
                store_name: Some(store_name),
                time_to_live: ttl_seconds,
                sync_option: None,
            },
        }
    }
}

#[derive(Clone)]
pub struct UpdateTimeseriesAnalyticalStoreOperation {
    client: OtsClient,
    request: UpdateTimeseriesAnalyticalStoreRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(UpdateTimeseriesAnalyticalStoreOperation);

impl UpdateTimeseriesAnalyticalStoreOperation {
    pub(crate) fn new(client: OtsClient, request: UpdateTimeseriesAnalyticalStoreRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;
        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::UpdateTimeseriesAnalyticalStoreRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::UpdateTimeseriesAnalyticalStore,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        resp.bytes().await?;
        Ok(())
    }
}

use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    protos::timeseries::AnalyticalStoreSyncType,
    timeseries_model::rules::{validate_analytical_store_name, validate_timeseries_table_name, MIN_ANALYTICAL_STORE_TTL_SECONDS},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 为已存在的时序表创建一个时序分析存储用于低成本存储时序数据以及查询与分析时序数据
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/createtimeseriesanalyticalstore-api-of-tablestore>
#[derive(Debug, Clone, Default)]
pub struct CreateTimeseriesAnalyticalStoreRequest {
    /// 时序表名
    pub table_name: String,

    /// 分析存储名
    pub store_name: String,

    /// 分析存储数据保留时间。取值必须大于等于 `2592000` 秒（即 `30` 天）或者必须为 `-1`（数据永不过期）
    pub ttl_seconds: Option<i32>,

    /// 分析存储同步方式
    pub sync_option: Option<AnalyticalStoreSyncType>,
}

impl CreateTimeseriesAnalyticalStoreRequest {
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

    /// 设置分析存储同步方式
    pub fn sync_option(mut self, sync_option: AnalyticalStoreSyncType) -> Self {
        self.sync_option = Some(sync_option);

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

impl From<CreateTimeseriesAnalyticalStoreRequest> for crate::protos::timeseries::CreateTimeseriesAnalyticalStoreRequest {
    fn from(value: CreateTimeseriesAnalyticalStoreRequest) -> Self {
        let CreateTimeseriesAnalyticalStoreRequest {
            table_name,
            store_name,
            ttl_seconds,
            sync_option,
        } = value;

        Self {
            table_name,
            analytical_store: Some(crate::protos::timeseries::TimeseriesAnalyticalStore {
                store_name: Some(store_name),
                time_to_live: ttl_seconds,
                sync_option: sync_option.map(|o| o as i32),
            }),
        }
    }
}

#[derive(Clone)]
pub struct CreateTimeseriesAnalyticalStoreOperation {
    client: OtsClient,
    request: CreateTimeseriesAnalyticalStoreRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(CreateTimeseriesAnalyticalStoreOperation);

impl CreateTimeseriesAnalyticalStoreOperation {
    pub(crate) fn new(client: OtsClient, request: CreateTimeseriesAnalyticalStoreRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;
        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::CreateTimeseriesAnalyticalStoreRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::CreateTimeseriesAnalyticalStore,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        resp.bytes().await?;
        Ok(())
    }
}

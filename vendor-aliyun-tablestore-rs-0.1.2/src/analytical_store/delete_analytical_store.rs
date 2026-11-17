use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    timeseries_model::rules::{validate_analytical_store_name, validate_timeseries_table_name},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 删除一个时序分析存储
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/deletetimeseriesanalyticalstore-api-of-tablestore>
#[derive(Debug, Default, Clone)]
pub struct DeleteTimeseriesAnalyticalStoreRequest {
    /// 时序表名称
    pub table_name: String,

    /// 分析存储名称
    pub store_name: String,

    /// 是否级联删除分析存储关联的 SQL 映射表
    pub drop_mapping_table: Option<bool>,
}

impl DeleteTimeseriesAnalyticalStoreRequest {
    pub fn new(table_name: &str, store_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            store_name: store_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置是否级联删除分析存储关联的 SQL 映射表
    pub fn drop_mapping_table(mut self, drop_mapping_table: bool) -> Self {
        self.drop_mapping_table = Some(drop_mapping_table);

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

        Ok(())
    }
}

impl From<DeleteTimeseriesAnalyticalStoreRequest> for crate::protos::timeseries::DeleteTimeseriesAnalyticalStoreRequest {
    fn from(value: DeleteTimeseriesAnalyticalStoreRequest) -> Self {
        let DeleteTimeseriesAnalyticalStoreRequest {
            table_name,
            store_name,
            drop_mapping_table,
        } = value;

        Self {
            table_name,
            store_name,
            drop_mapping_table,
        }
    }
}

#[derive(Clone)]
pub struct DeleteTimeseriesAnalyticalStoreOperation {
    client: OtsClient,
    request: DeleteTimeseriesAnalyticalStoreRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(DeleteTimeseriesAnalyticalStoreOperation);

impl DeleteTimeseriesAnalyticalStoreOperation {
    pub(crate) fn new(client: OtsClient, request: DeleteTimeseriesAnalyticalStoreRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::DeleteTimeseriesAnalyticalStoreRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::DeleteTimeseriesAnalyticalStore,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        resp.bytes().await?;

        Ok(())
    }
}

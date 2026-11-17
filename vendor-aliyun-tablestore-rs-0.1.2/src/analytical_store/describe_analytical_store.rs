use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    timeseries_model::rules::{validate_analytical_store_name, validate_timeseries_table_name},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 查询时序分析存储描述信息，例如分析存储配置信息、分析存储同步状态、分析存储大小等
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/describe-timeseries-analytical-store>
#[derive(Clone)]
pub struct DescribeTimeseriesAnalyticalStoreOperation {
    client: OtsClient,
    table_name: String,
    store_name: String,
    options: OtsRequestOptions,
}

add_per_request_options!(DescribeTimeseriesAnalyticalStoreOperation);

impl DescribeTimeseriesAnalyticalStoreOperation {
    pub(crate) fn new(client: OtsClient, table_name: &str, store_name: &str) -> Self {
        Self {
            client,
            table_name: table_name.to_string(),
            store_name: store_name.to_string(),
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<crate::protos::timeseries::DescribeTimeseriesAnalyticalStoreResponse> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalie timeseries table name: {}", self.table_name)));
        }

        if !validate_analytical_store_name(&self.store_name) {
            return Err(OtsError::ValidationFailed(format!(
                "invalid timeseries table analytical store name: {}",
                self.store_name
            )));
        }

        let Self {
            client,
            table_name,
            store_name,
            options,
        } = self;

        let msg = crate::protos::timeseries::DescribeTimeseriesAnalyticalStoreRequest { table_name, store_name };

        let req = OtsRequest {
            operation: OtsOp::DescribeTimeseriesAnalyticalStore,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        Ok(crate::protos::timeseries::DescribeTimeseriesAnalyticalStoreResponse::decode(
            resp.bytes().await?,
        )?)
    }
}

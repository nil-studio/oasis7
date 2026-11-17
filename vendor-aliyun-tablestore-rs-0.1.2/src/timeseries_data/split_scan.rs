use prost::Message;

use crate::{
    add_per_request_options, error::OtsError, timeseries_model::rules::validate_timeseries_table_name, OtsClient, OtsOp, OtsRequest, OtsRequestOptions,
    OtsResult,
};

/// 切分全量导出任务
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/splittimeseriesscantask>
#[derive(Debug, Default, Clone)]
pub struct SplitTimeseriesScanTaskRequest {
    /// 时序表名
    pub table_name: String,

    /// 度量名称
    pub measurement_name: Option<String>,

    /// 期望切分的任务数。服务端会根据该值切分任务个数，但实际切分的任务个数由服务端决定
    pub split_count_hint: u32,
}

impl SplitTimeseriesScanTaskRequest {
    pub fn new(table_name: &str, split_count_hint: u32) -> Self {
        Self {
            table_name: table_name.to_string(),
            split_count_hint,
            ..Default::default()
        }
    }

    /// 设置度量名称
    pub fn measurement_name(mut self, m_name: impl Into<String>) -> Self {
        self.measurement_name = Some(m_name.into());

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid timeseries table name: {}", self.table_name)));
        }

        if self.split_count_hint == 0 {
            return Err(OtsError::ValidationFailed("invalid split count hint. must be greater than 0".to_string()));
        }

        if self.split_count_hint > i32::MAX as u32 {
            return Err(OtsError::ValidationFailed(format!(
                "invalid split count hint: {}, too large for i32",
                self.split_count_hint
            )));
        }

        Ok(())
    }
}

impl From<SplitTimeseriesScanTaskRequest> for crate::protos::timeseries::SplitTimeseriesScanTaskRequest {
    fn from(value: SplitTimeseriesScanTaskRequest) -> Self {
        let SplitTimeseriesScanTaskRequest {
            table_name,
            measurement_name,
            split_count_hint,
        } = value;

        Self {
            table_name,
            measurement_name,
            split_count_hint: split_count_hint as i32,
        }
    }
}

#[derive(Clone)]
pub struct SplitTimeseriesScanTaskOperation {
    client: OtsClient,
    request: SplitTimeseriesScanTaskRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(SplitTimeseriesScanTaskOperation);

impl SplitTimeseriesScanTaskOperation {
    pub(crate) fn new(client: OtsClient, request: SplitTimeseriesScanTaskRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<crate::protos::timeseries::SplitTimeseriesScanTaskResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::SplitTimeseriesScanTaskRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::SplitTimeseriesScanTask,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        Ok(crate::protos::timeseries::SplitTimeseriesScanTaskResponse::decode(resp.bytes().await?)?)
    }
}

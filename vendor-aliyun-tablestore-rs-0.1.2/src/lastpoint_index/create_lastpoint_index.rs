use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    timeseries_model::rules::{validate_lastpoint_index_name, validate_timeseries_table_name},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 创建Lastpoint索引
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/createtimeserieslastpointindex>
#[derive(Debug, Clone, Default)]
pub struct CreateTimeseriesLastpointIndexRequest {
    /// 时序表名称
    pub table_name: String,

    /// Lastpoint索引名称
    pub index_name: String,

    /// 是否包含存量数据
    pub include_base_data: Option<bool>,

    /// 是否创建在宽表上
    pub on_wide_column_table: Option<bool>,

    /// 主键列
    pub primary_key_names: Vec<String>,
}

impl CreateTimeseriesLastpointIndexRequest {
    pub fn new(table_name: &str, index_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            index_name: index_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置是否包含存量数据
    pub fn include_base_data(mut self, include_base_data: bool) -> Self {
        self.include_base_data = Some(include_base_data);

        self
    }

    /// 设置是否在宽表上创建索引
    pub fn on_wide_column_table(mut self, on_wide_column_table: bool) -> Self {
        self.on_wide_column_table = Some(on_wide_column_table);

        self
    }

    /// 增加一个主键列名称
    pub fn primary_key_name(mut self, pk_name: impl Into<String>) -> Self {
        self.primary_key_names.push(pk_name.into());

        self
    }

    /// 设置主键列名称
    pub fn primary_key_names(mut self, pk_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.primary_key_names = pk_names.into_iter().map(|s| s.into()).collect();

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalie timeseries table name: {}", self.table_name)));
        }

        if !validate_lastpoint_index_name(&self.index_name) {
            return Err(OtsError::ValidationFailed(format!(
                "invalid timeseries table lastpoint index name: {}",
                self.index_name
            )));
        }

        Ok(())
    }
}

impl From<CreateTimeseriesLastpointIndexRequest> for crate::protos::timeseries::CreateTimeseriesLastpointIndexRequest {
    fn from(value: CreateTimeseriesLastpointIndexRequest) -> Self {
        let CreateTimeseriesLastpointIndexRequest {
            table_name,
            index_name,
            include_base_data,
            on_wide_column_table,
            primary_key_names,
        } = value;

        Self {
            main_table_name: table_name,
            index_table_name: index_name,
            include_base_data,
            create_on_wide_column_table: on_wide_column_table,
            index_primary_key_names: primary_key_names,
        }
    }
}

#[derive(Clone)]
pub struct CreateTimeseriesLastpointIndexOperation {
    client: OtsClient,
    request: CreateTimeseriesLastpointIndexRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(CreateTimeseriesLastpointIndexOperation);

impl CreateTimeseriesLastpointIndexOperation {
    pub(crate) fn new(client: OtsClient, request: CreateTimeseriesLastpointIndexRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::CreateTimeseriesLastpointIndexRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::CreateTimeseriesLastpointIndex,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        resp.bytes().await?;

        Ok(())
    }
}

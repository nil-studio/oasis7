use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    timeseries_model::{rules::validate_timeseries_table_name, MetaQuery, TimeseriesMeta, SUPPORTED_TABLE_VERSION},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 检索时间线元数据
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/querytimeseriesmeta>
#[derive(Debug, Clone)]
pub struct QueryTimeseriesMetaRequest {
    /// 时序表名
    pub table_name: String,

    /// 查询条件
    pub condition: MetaQuery,

    /// 是否获取符合条件总行数
    pub get_total_hit: Option<bool>,

    /// 用于继续获取剩余数据的标识
    pub token: Option<Vec<u8>>,

    /// 最多返回的行数限制
    pub limit: Option<u32>,
}

impl QueryTimeseriesMetaRequest {
    pub fn new(table_name: &str, condition: MetaQuery) -> Self {
        Self {
            table_name: table_name.to_string(),
            condition,
            get_total_hit: None,
            token: None,
            limit: None,
        }
    }

    /// 设置是否获取全部行数
    pub fn get_total_hit(mut self, with_total_hit: bool) -> Self {
        self.get_total_hit = Some(with_total_hit);

        self
    }

    /// 设置翻页 token
    pub fn token(mut self, token: impl Into<Vec<u8>>) -> Self {
        self.token = Some(token.into());

        self
    }

    /// 设置最多返回条数
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if let Some(n) = self.limit {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("invalid limit: {}", n)));
            }
        }

        self.condition.validate()?;

        Ok(())
    }
}

impl From<QueryTimeseriesMetaRequest> for crate::protos::timeseries::QueryTimeseriesMetaRequest {
    fn from(value: QueryTimeseriesMetaRequest) -> Self {
        let QueryTimeseriesMetaRequest {
            table_name,
            condition,
            get_total_hit,
            token,
            limit,
        } = value;

        Self {
            table_name,
            condition: Some(crate::protos::timeseries::MetaQueryCondition::from(condition)),
            get_total_hit,
            token,
            limit: limit.map(|n| n as i32),
            supported_table_version: Some(SUPPORTED_TABLE_VERSION),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueryTimeseriesMetaResponse {
    /// 时间线元数据列表
    pub metas: Vec<TimeseriesMeta>,

    /// 只有在请求中设置 `get_total_hit` 为 true 时才会返回符合条件的总行数
    pub total_hit: Option<u64>,

    /// 用于获取剩余数据的标识
    pub next_token: Option<Vec<u8>>,
}

impl From<crate::protos::timeseries::QueryTimeseriesMetaResponse> for QueryTimeseriesMetaResponse {
    fn from(value: crate::protos::timeseries::QueryTimeseriesMetaResponse) -> Self {
        let crate::protos::timeseries::QueryTimeseriesMetaResponse {
            timeseries_metas,
            total_hit,
            next_token,
        } = value;

        Self {
            metas: timeseries_metas.into_iter().map(TimeseriesMeta::from).collect(),
            total_hit: if let Some(n) = total_hit {
                // 如果在请求中没有要求返回命中行数，服务会返回 `Some(-1)`
                if n >= 0 {
                    Some(n as u64)
                } else {
                    None
                }
            } else {
                None
            },
            next_token,
        }
    }
}

#[derive(Clone)]
pub struct QueryTimeseriesMetaOperation {
    client: OtsClient,
    request: QueryTimeseriesMetaRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(QueryTimeseriesMetaOperation);

impl QueryTimeseriesMetaOperation {
    pub(crate) fn new(client: OtsClient, request: QueryTimeseriesMetaRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<QueryTimeseriesMetaResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::QueryTimeseriesMetaRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::QueryTimeseriesMeta,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        let resp_msg = crate::protos::timeseries::QueryTimeseriesMetaResponse::decode(resp.bytes().await?)?;

        Ok(resp_msg.into())
    }
}

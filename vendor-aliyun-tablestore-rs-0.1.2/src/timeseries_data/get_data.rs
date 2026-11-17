use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    model::decode_plainbuf_rows,
    protos::plain_buffer::MASK_HEADER,
    timeseries_model::{rules::validate_timeseries_table_name, TimeseriesFieldToGet, TimeseriesKey, TimeseriesRow, SUPPORTED_TABLE_VERSION},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 查询某个时间线的数据
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/gettimeseriesdata>
#[derive(Debug, Default, Clone)]
pub struct GetTimeseriesDataRequest {
    /// 表名
    pub table_name: String,

    /// 时间线标识
    pub key: TimeseriesKey,

    /// 开始时间。格式为微秒单位时间戳（从 1970-01-01 00:00:00 UTC 计算起的微秒数）
    pub begin_time_us: u64,

    /// 结束时间。格式为微秒单位时间戳（从 1970-01-01 00:00:00 UTC 计算起的微秒数）
    pub end_time_us: u64,

    /// 特定时间。格式为微秒单位时间戳（从 1970-01-01 00:00:00 UTC 计算起的微秒数）
    pub specific_time_us: Option<u64>,

    /// 用于继续获取剩余数据的标识
    pub token: Option<Vec<u8>>,

    /// 最多返回的行数
    pub limit: Option<u32>,

    /// 是否按照时间倒序读取。默认为正序读取
    pub backward: bool,

    /// 指定读取部分数据列
    pub fields_to_get: Vec<TimeseriesFieldToGet>,
}

impl GetTimeseriesDataRequest {
    pub fn new(table_name: &str, key: TimeseriesKey) -> Self {
        Self {
            table_name: table_name.to_string(),
            key,
            ..Default::default()
        }
    }

    /// 设置开始时间。微秒时间戳（从 1970-01-01 00:00:00 UTC 计算起的微秒数）
    pub fn begin_time_us(mut self, begin_time: u64) -> Self {
        self.begin_time_us = begin_time;
        self
    }

    /// 设置结束时间。微秒时间戳（从 1970-01-01 00:00:00 UTC 计算起的微秒数）
    pub fn end_time_us(mut self, end_time: u64) -> Self {
        self.end_time_us = end_time;
        self
    }

    /// 设置指定时间。微秒时间戳（从 1970-01-01 00:00:00 UTC 计算起的微秒数）
    pub fn specific_time_us(mut self, specific_time: u64) -> Self {
        self.specific_time_us = Some(specific_time);
        self
    }

    /// 设置获取更多数据的 token
    pub fn token(mut self, token: Vec<u8>) -> Self {
        self.token = Some(token);
        self
    }

    /// 设置最多返回行数
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// 设置是否按照时间倒序
    pub fn backward(mut self, backward: bool) -> Self {
        self.backward = backward;
        self
    }

    /// 添加一个要设置的列
    pub fn field_to_get(mut self, field: TimeseriesFieldToGet) -> Self {
        self.fields_to_get.push(field);
        self
    }

    /// 设置要获取的列
    pub fn fields_to_get(mut self, fields_to_get: impl IntoIterator<Item = TimeseriesFieldToGet>) -> Self {
        self.fields_to_get = fields_to_get.into_iter().collect();
        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        self.key.validate()?;

        if self.begin_time_us > i64::MAX as u64 {
            return Err(OtsError::ValidationFailed(format!("invalid begin_time_us: {}", self.begin_time_us)));
        }

        if self.end_time_us > i64::MAX as u64 {
            return Err(OtsError::ValidationFailed(format!("invalid end_time_us: {}", self.end_time_us)));
        }

        if self.end_time_us == 0 {
            return Err(OtsError::ValidationFailed("end_time_us must be greater than 0".to_string()));
        }

        if let Some(n) = self.limit {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("limit is too large: {}", n)));
            }
        }

        Ok(())
    }
}

impl From<GetTimeseriesDataRequest> for crate::protos::timeseries::GetTimeseriesDataRequest {
    fn from(value: GetTimeseriesDataRequest) -> Self {
        let GetTimeseriesDataRequest {
            table_name,
            key,
            begin_time_us,
            end_time_us,
            specific_time_us,
            token,
            limit,
            backward,
            fields_to_get,
        } = value;

        Self {
            table_name,
            time_series_key: key.into(),
            begin_time: Some(begin_time_us as i64),
            end_time: Some(end_time_us as i64),
            specific_time: specific_time_us.map(|t| t as i64),
            token,
            limit: limit.map(|n| n as i32),
            backward: Some(backward),
            fields_to_get: fields_to_get.into_iter().map(|field| field.into()).collect(),
            supported_table_version: Some(SUPPORTED_TABLE_VERSION),
        }
    }
}

/// 获取时序表数据响应
#[derive(Debug, Clone)]
pub struct GetTimeseriesDataResponse {
    /// 行数据
    pub rows: Vec<TimeseriesRow>,

    /// 分页 token
    pub next_token: Option<Vec<u8>>,
}

impl TryFrom<crate::protos::timeseries::GetTimeseriesDataResponse> for GetTimeseriesDataResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::timeseries::GetTimeseriesDataResponse) -> Result<Self, Self::Error> {
        let crate::protos::timeseries::GetTimeseriesDataResponse { rows_data, next_token } = value;

        // Returned bytes with plainbuf encoding
        let plainbuf_rows = decode_plainbuf_rows(rows_data, MASK_HEADER)?;

        Ok(Self {
            rows: plainbuf_rows.into_iter().map(TimeseriesRow::from).collect(),
            next_token,
        })
    }
}

/// 查询某个时间线的数据
#[derive(Clone)]
pub struct GetTimeseriesDataOperation {
    client: OtsClient,
    request: GetTimeseriesDataRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(GetTimeseriesDataOperation);

impl GetTimeseriesDataOperation {
    pub(crate) fn new(client: OtsClient, request: GetTimeseriesDataRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<GetTimeseriesDataResponse> {
        self.request.validate()?;
        let Self { client, request, options } = self;
        let msg = crate::protos::timeseries::GetTimeseriesDataRequest::from(request);
        let req = OtsRequest {
            operation: OtsOp::GetTimeseriesData,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        let resp_msg = crate::protos::timeseries::GetTimeseriesDataResponse::decode(resp.bytes().await?)?;

        resp_msg.try_into()
    }
}

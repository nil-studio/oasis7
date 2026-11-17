use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    model::decode_plainbuf_rows,
    protos::{
        plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
        timeseries::RowsSerializeType,
    },
    timeseries_model::{rules::validate_timeseries_table_name, TimeseriesFieldToGet, TimeseriesRow, SUPPORTED_TABLE_VERSION},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

const MAX_ROWS: u32 = 5000;

/// 扫描时序数据
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/scantimeseriesdata>
#[derive(Debug, Default, Clone)]
pub struct ScanTimeseriesDataRequest {
    /// 时序表名称
    pub table_name: String,

    /// 通过 `SplitTimeseriesScanTask` 接口返回的 SplitInfo
    pub split_info: Option<Vec<u8>>,

    /// 开始时间，微秒
    pub start_time_us: Option<u64>,

    /// 结束时间，微秒
    pub end_time_us: Option<u64>,

    /// 指定读取部分数据列
    pub fields_to_get: Vec<TimeseriesFieldToGet>,

    /// 每次最多返回的行数，最大值为 `5000`，默认值为 `5000`
    pub limit: Option<u32>,

    /// 用于继续获取剩余数据的标识
    pub token: Option<Vec<u8>>,
}

impl ScanTimeseriesDataRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置 Split info
    pub fn split_info(mut self, info: impl Into<Vec<u8>>) -> Self {
        self.split_info = Some(info.into());

        self
    }

    /// 设置开始时间，微秒，大于等于 0
    pub fn start_time_us(mut self, ts_us: u64) -> Self {
        self.start_time_us = Some(ts_us);

        self
    }

    /// 设置结束时间，微秒，大于开始时间
    pub fn end_time_us(mut self, ts_us: u64) -> Self {
        self.end_time_us = Some(ts_us);

        self
    }

    /// 添加一个要获取的列
    pub fn field_to_get(mut self, field: TimeseriesFieldToGet) -> Self {
        self.fields_to_get.push(field);

        self
    }

    /// 设置要获取的列
    pub fn fields_to_get(mut self, fields: impl IntoIterator<Item = TimeseriesFieldToGet>) -> Self {
        self.fields_to_get = fields.into_iter().collect();

        self
    }

    /// 设置返回的行数。最多 5000
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);

        self
    }

    /// 设置获取剩余数据的标识
    pub fn token(mut self, token: impl Into<Vec<u8>>) -> Self {
        self.token = Some(token.into());

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid timeseries table name: {}", self.table_name)));
        }

        if let Some(n) = self.start_time_us {
            if n > i64::MAX as u64 {
                return Err(OtsError::ValidationFailed(format!("start times (us): {} is too large for i64", n)));
            }
        }

        if let Some(n) = self.end_time_us {
            if n > i64::MAX as u64 {
                return Err(OtsError::ValidationFailed(format!("end times (us): {} is too large for i64", n)));
            }
        }

        if let (Some(start), Some(end)) = (self.start_time_us, self.end_time_us) {
            if end <= start {
                return Err(OtsError::ValidationFailed(format!(
                    "end time (us): {} must be large than begin time (us): {}",
                    end, start
                )));
            }
        }

        if let Some(n) = self.limit {
            if n > MAX_ROWS {
                return Err(OtsError::ValidationFailed(format!(
                    "limit: {} is too large. maximum limit allowed: {}",
                    n, MAX_ROWS
                )));
            }
        }

        Ok(())
    }
}

impl From<ScanTimeseriesDataRequest> for crate::protos::timeseries::ScanTimeseriesDataRequest {
    fn from(value: ScanTimeseriesDataRequest) -> Self {
        let ScanTimeseriesDataRequest {
            table_name,
            split_info,
            start_time_us,
            end_time_us,
            fields_to_get,
            limit,
            token,
        } = value;

        Self {
            table_name,
            split_info,
            start_time_us: start_time_us.map(|n| n as i64),
            end_time_us: end_time_us.map(|n| n as i64),
            fields_to_get: fields_to_get.into_iter().map(crate::protos::timeseries::TimeseriesFieldsToGet::from).collect(),
            limit: limit.map(|n| n as i32),
            data_serialize_type: Some(RowsSerializeType::RstPlainBuffer as i32),
            token,
            supported_table_version: Some(SUPPORTED_TABLE_VERSION),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScanTimeseriesDataResponse {
    pub rows: Vec<TimeseriesRow>,
    pub next_token: Option<Vec<u8>>,
}

impl TryFrom<crate::protos::timeseries::ScanTimeseriesDataResponse> for ScanTimeseriesDataResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::timeseries::ScanTimeseriesDataResponse) -> Result<Self, Self::Error> {
        let crate::protos::timeseries::ScanTimeseriesDataResponse {
            data_serialize_type: _,
            data,
            next_token,
        } = value;

        let rows = if !data.is_empty() {
            decode_plainbuf_rows(data, MASK_HEADER | MASK_ROW_CHECKSUM)?
        } else {
            vec![]
        };

        Ok(Self {
            rows: rows.into_iter().map(TimeseriesRow::from).collect(),
            next_token,
        })
    }
}

#[derive(Clone)]
pub struct ScanTimeseriesDataOperation {
    client: OtsClient,
    request: ScanTimeseriesDataRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(ScanTimeseriesDataOperation);

impl ScanTimeseriesDataOperation {
    pub(crate) fn new(client: OtsClient, request: ScanTimeseriesDataRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<ScanTimeseriesDataResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::ScanTimeseriesDataRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::ScanTimeseriesData,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        let resp_msg = crate::protos::timeseries::ScanTimeseriesDataResponse::decode(resp.bytes().await?)?;

        resp_msg.try_into()
    }
}

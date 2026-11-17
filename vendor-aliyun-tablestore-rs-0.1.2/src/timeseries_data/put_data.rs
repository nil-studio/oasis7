use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    protos::timeseries::MetaUpdateMode,
    timeseries_model::{self, encode_flatbuf_rows, rules::validate_timeseries_table_name, TimeseriesRow, SUPPORTED_TABLE_VERSION},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 写入时序数据。目前暂时只支持 flat buffer 编码。
///
/// 神奇的事情：官方文档说支持 Plain Buffer 和 Flat Buffer 编码。但是
///
/// - Java SDK 中只支持 Flat Buffer 编码；
/// - Go SDK 中支持 Plain Buffer 和 Proto Buffer 编码。而且，`timeseries.proto` 文件内容和 Java SDK 中的也不一样
///
/// 目前在写入数据的之后，暂时不支持设置 `meta_cache_update_time`，交给系统默认处理。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/puttimeseriesdata>
#[derive(Debug, Default, Clone)]
pub struct PutTimeseriesDataRequest {
    /// 时序表名称
    pub table_name: String,

    /// 要插入的行数据
    pub rows: Vec<TimeseriesRow>,

    /// 元数据更新模式
    pub meta_update_mode: Option<MetaUpdateMode>,
}

impl PutTimeseriesDataRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 添加一行数据
    pub fn row(mut self, row: TimeseriesRow) -> Self {
        self.rows.push(row);

        self
    }

    /// 设置所有的行
    pub fn rows(mut self, rows: impl IntoIterator<Item = TimeseriesRow>) -> Self {
        self.rows = rows.into_iter().collect();

        self
    }

    /// 设置元数据更新模式
    pub fn meta_update_mode(mut self, meta_update_mode: MetaUpdateMode) -> Self {
        self.meta_update_mode = Some(meta_update_mode);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if self.rows.is_empty() {
            return Err(OtsError::ValidationFailed("can not put empty rows to timeseries table".to_string()));
        }

        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid timeseries table name: {}", self.table_name)));
        }

        if self.rows.len() > timeseries_model::rules::MAX_ROW_COUNT {
            return Err(OtsError::ValidationFailed(format!(
                "rows count exceeds max rows count allowed: {}",
                timeseries_model::rules::MAX_ROW_COUNT
            )));
        }

        for row in &self.rows {
            row.validate()?;
        }

        Ok(())
    }
}

impl From<PutTimeseriesDataRequest> for crate::protos::timeseries::PutTimeseriesDataRequest {
    fn from(value: PutTimeseriesDataRequest) -> Self {
        let PutTimeseriesDataRequest {
            table_name,
            rows,
            meta_update_mode,
        } = value;

        let bytes = encode_flatbuf_rows(rows.as_slice()).unwrap();

        let checksum = crc32c::crc32c(&bytes);

        Self {
            table_name,
            rows_data: crate::protos::timeseries::TimeseriesRows {
                r#type: crate::protos::timeseries::RowsSerializeType::RstFlatBuffer as i32,
                flatbuffer_crc32c: Some(checksum as i32),
                rows_data: bytes,
            },
            meta_update_mode: meta_update_mode.map(|m| m as i32),
            supported_table_version: Some(SUPPORTED_TABLE_VERSION),
        }
    }
}

#[derive(Clone)]
pub struct PutTimeseriesDataOperation {
    client: OtsClient,
    request: PutTimeseriesDataRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(PutTimeseriesDataOperation);

impl PutTimeseriesDataOperation {
    pub(crate) fn new(client: OtsClient, request: PutTimeseriesDataRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<crate::protos::timeseries::PutTimeseriesDataResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::PutTimeseriesDataRequest::from(request);

        if msg.rows_data.rows_data.len() > crate::timeseries_model::rules::MAX_DATA_SIZE {
            return Err(OtsError::ValidationFailed(format!(
                "data size: {} exceeds max data size allowed: {}",
                msg.rows_data.rows_data.len(),
                crate::timeseries_model::rules::MAX_DATA_SIZE
            )));
        }

        let req = OtsRequest {
            operation: OtsOp::PutTimeseriesData,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        Ok(crate::protos::timeseries::PutTimeseriesDataResponse::decode(resp.bytes().await?)?)
    }
}

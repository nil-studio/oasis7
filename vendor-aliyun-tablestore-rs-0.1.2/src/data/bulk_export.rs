use std::{collections::HashSet, io::Cursor};

use byteorder::{LittleEndian, ReadBytesExt};
use prost::Message;

use crate::model::rules::validate_table_name;
use crate::{
    add_per_request_options,
    error::OtsError,
    model::{Filter, PrimaryKey, PrimaryKeyColumn, Row},
    protos::{
        plain_buffer::{HEADER, MASK_HEADER, MASK_ROW_CHECKSUM},
        simple_row_matrix::SimpleRowMatrix,
        ConsumedCapacity, DataBlockType,
    },
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 接口批量导出数据。数据编码采用 Simple Row Matrix 格式
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/bulkexport>
#[derive(Debug, Clone)]
pub struct BulkExportRequest {
    /// 表名
    pub table_name: String,

    /// 要返回的列。使用 DBT_SIMPLE_ROW_MATRIX 编码方式的返回值时，必须至少设置一个返回列
    pub columns_to_get: HashSet<String>,

    /// 起始主键，包含
    pub inclusive_start_primary_key: PrimaryKey,

    /// 结束主键，不包含
    pub exclusive_end_primary_key: PrimaryKey,

    /// 过滤器
    pub filter: Option<Filter>,
}

impl Default for BulkExportRequest {
    fn default() -> Self {
        Self {
            table_name: "".to_string(),
            columns_to_get: HashSet::new(),
            inclusive_start_primary_key: PrimaryKey::new(),
            exclusive_end_primary_key: PrimaryKey::new(),
            filter: None,
        }
    }
}

impl BulkExportRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 添加主键查询范围
    pub fn primary_key_range(mut self, start_pk: PrimaryKey, end_pk: PrimaryKey) -> Self {
        self.inclusive_start_primary_key = start_pk;
        self.exclusive_end_primary_key = end_pk;

        self
    }

    pub fn start_primary_key(mut self, pk: PrimaryKey) -> Self {
        self.inclusive_start_primary_key = pk;

        self
    }

    /// 添加一个开始主键列
    pub fn start_primary_key_column(mut self, pk_col: PrimaryKeyColumn) -> Self {
        self.inclusive_start_primary_key.columns.push(pk_col);

        self
    }

    /// 设置开始主键列
    pub fn start_primary_key_columns(mut self, pk_cols: impl IntoIterator<Item = PrimaryKeyColumn>) -> Self {
        self.inclusive_start_primary_key = PrimaryKey {
            columns: pk_cols.into_iter().collect(),
        };

        self
    }

    /// 添加字符串类型的开始主键查询值。本次范围读取的起始主键，如果该行存在，则响应中一定会包含此行。
    pub fn start_primary_key_column_string(mut self, name: &str, value: impl Into<String>) -> Self {
        self.inclusive_start_primary_key.columns.push(PrimaryKeyColumn::from_string(name, value));
        self
    }

    /// 添加整数类型的开始主键查询值。本次范围读取的起始主键，如果该行存在，则响应中一定会包含此行。
    pub fn start_primary_key_column_integer(mut self, name: &str, value: i64) -> Self {
        self.inclusive_start_primary_key.columns.push(PrimaryKeyColumn::from_integer(name, value));

        self
    }

    /// 添加二进制类型的开始主键查询值。本次范围读取的起始主键，如果该行存在，则响应中一定会包含此行。
    pub fn start_primary_key_column_binary(mut self, name: &str, value: impl Into<Vec<u8>>) -> Self {
        self.inclusive_start_primary_key.columns.push(PrimaryKeyColumn::from_binary(name, value));

        self
    }

    /// 添加无穷小值开始主键
    pub fn start_primary_key_column_inf_min(mut self, name: &str) -> Self {
        self.inclusive_start_primary_key.columns.push(PrimaryKeyColumn::inf_min(name));

        self
    }

    /// 添加无穷大值开始主键
    pub fn start_primary_key_column_inf_max(mut self, name: &str) -> Self {
        self.inclusive_start_primary_key.columns.push(PrimaryKeyColumn::inf_max(name));

        self
    }

    /// 设置结束主键列
    pub fn end_primary_key(mut self, pk: PrimaryKey) -> Self {
        self.exclusive_end_primary_key = pk;

        self
    }

    /// 添加一个结束主键列
    pub fn end_primary_key_column(mut self, pk_col: PrimaryKeyColumn) -> Self {
        self.exclusive_end_primary_key.columns.push(pk_col);

        self
    }

    /// 设置结束主键列
    pub fn end_primary_key_columns(mut self, pk_cols: impl IntoIterator<Item = PrimaryKeyColumn>) -> Self {
        self.exclusive_end_primary_key = PrimaryKey {
            columns: pk_cols.into_iter().collect(),
        };

        self
    }

    /// 添加字符串类型的结束主键查询值。无论该行是否存在，则响应中一定不会包含此行。
    pub fn end_primary_key_column_string(mut self, name: &str, value: impl Into<String>) -> Self {
        self.exclusive_end_primary_key.columns.push(PrimaryKeyColumn::from_string(name, value));
        self
    }

    /// 添加整数类型的结束主键查询值。无论该行是否存在，则响应中一定不会包含此行。
    pub fn end_primary_key_column_integer(mut self, name: &str, value: i64) -> Self {
        self.exclusive_end_primary_key.columns.push(PrimaryKeyColumn::from_integer(name, value));

        self
    }

    /// 添加二进制类型的结束主键查询值。无论该行是否存在，则响应中一定不会包含此行。
    pub fn end_primary_key_column_binary(mut self, name: &str, value: impl Into<Vec<u8>>) -> Self {
        self.exclusive_end_primary_key.columns.push(PrimaryKeyColumn::from_binary(name, value));

        self
    }

    /// 添加无穷小值结束主键
    pub fn end_primary_key_column_inf_min(mut self, name: &str) -> Self {
        self.exclusive_end_primary_key.columns.push(PrimaryKeyColumn::inf_min(name));

        self
    }

    /// 添加无穷大值结束主键
    pub fn end_primary_key_column_inf_max(mut self, name: &str) -> Self {
        self.exclusive_end_primary_key.columns.push(PrimaryKeyColumn::inf_max(name));

        self
    }

    /// 需要返回的全部列的列名。如果为空，则返回指定行的所有列。`columns_to_get` 个数不应超过128个。
    /// 如果指定的列不存在，则不会返回指定列的数据；如果给出了重复的列名，返回结果只会包含一次指定列。
    pub fn column_to_get(mut self, name: &str) -> Self {
        self.columns_to_get.insert(name.to_string());

        self
    }

    /// 设置需要返回的列
    pub fn columns_to_get(mut self, names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.columns_to_get = names.into_iter().map(|s| s.into()).collect();

        self
    }

    /// 设置过滤器
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);

        self
    }

    /// 发送请求前验证
    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if self.inclusive_start_primary_key.columns.is_empty() || self.exclusive_end_primary_key.columns.is_empty() {
            return Err(OtsError::ValidationFailed("invalid primary key: empty primary key columns".to_string()));
        }

        if self.columns_to_get.is_empty() {
            return Err(OtsError::ValidationFailed(
                "columns to get must be set when using simple row matrix data block type".to_string(),
            ));
        }

        Ok(())
    }
}

impl From<BulkExportRequest> for crate::protos::BulkExportRequest {
    fn from(value: BulkExportRequest) -> Self {
        let BulkExportRequest {
            table_name,
            columns_to_get,
            inclusive_start_primary_key,
            exclusive_end_primary_key,
            filter,
        } = value;

        crate::protos::BulkExportRequest {
            table_name,
            columns_to_get: columns_to_get.into_iter().collect(),
            inclusive_start_primary_key: inclusive_start_primary_key.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM),
            exclusive_end_primary_key: exclusive_end_primary_key.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM),
            filter: filter.map(|f| f.into_protobuf_bytes()),
            data_block_type_hint: Some(DataBlockType::DbtSimpleRowMatrix as i32),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct BulkExportResponse {
    pub consumed: ConsumedCapacity,
    pub rows: Vec<Row>,
    pub next_start_primary_key: Option<PrimaryKey>,

    /// 先不对外暴露这个属性
    #[allow(dead_code)]
    data_block_type: DataBlockType,
}

impl TryFrom<crate::protos::BulkExportResponse> for BulkExportResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::BulkExportResponse) -> Result<Self, Self::Error> {
        let crate::protos::BulkExportResponse {
            consumed,
            rows: rows_bytes,
            next_start_primary_key,
            data_block_type,
        } = value;

        let data_block_type = match data_block_type {
            Some(0) => DataBlockType::DbtPlainBuffer,
            _ => DataBlockType::DbtSimpleRowMatrix,
        };

        let rows = if !rows_bytes.is_empty() {
            match data_block_type {
                DataBlockType::DbtPlainBuffer => {
                    let mut rows = vec![];
                    let mut cursor = Cursor::new(rows_bytes);
                    let header = cursor.read_u32::<LittleEndian>()?;

                    if header != HEADER {
                        return Err(OtsError::PlainBufferError(format!("invalid message header: {}", header)));
                    }

                    loop {
                        if cursor.position() as usize >= cursor.get_ref().len() - 1 {
                            break;
                        }

                        let row = Row::read_plain_buffer(&mut cursor)?;
                        rows.push(row);
                    }

                    rows
                }
                DataBlockType::DbtSimpleRowMatrix => SimpleRowMatrix::new(rows_bytes).get_rows()?,
            }
        } else {
            vec![]
        };

        let pk = if let Some(pk_bytes) = next_start_primary_key {
            if !pk_bytes.is_empty() {
                Some(Row::decode_plain_buffer(pk_bytes, MASK_HEADER | MASK_ROW_CHECKSUM)?.primary_key)
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            consumed,
            rows,
            next_start_primary_key: pk,
            data_block_type,
        })
    }
}

#[derive(Clone)]
pub struct BulkExportOperation {
    client: OtsClient,
    request: BulkExportRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(BulkExportOperation);

impl BulkExportOperation {
    pub(crate) fn new(client: OtsClient, request: BulkExportRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<BulkExportResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::BulkExportRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::BulkExport,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        let resp_msg = crate::protos::BulkExportResponse::decode(resp.bytes().await?)?;

        resp_msg.try_into()
    }
}

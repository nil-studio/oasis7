use std::collections::HashSet;

use prost::Message;

use crate::model::rules::validate_table_name;
use crate::{
    add_per_request_options,
    error::OtsError,
    model::{Filter, Row},
    protos::{
        plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
        Condition, ConsumedCapacity, OperationType, ReturnContent, ReturnType, RowExistenceExpectation,
    },
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 在BatchWriteRow操作中，表示要插入、更新和删除的一行信息。
#[derive(Debug, Default, Clone)]
pub struct RowInBatchWriteRowRequest {
    /// 操作类型。
    pub operation_type: OperationType,

    /// 要写入的行
    pub row: Row,

    /// 在数据写入前是否进行存在性检查。取值范围如下：
    ///
    /// - `Ignore`（默认）：不做行存在性检查。
    /// - `ExpectExist` ：期望行存在。
    /// - `ExpectNotExist` ：期望行不存在。
    pub row_condition: RowExistenceExpectation,

    /// 进行行存在性检查的时候，可以附加列过滤器
    pub column_condition: Option<Filter>,

    /// 返回数据设置。目前仅支持返回主键，主要用于主键列自增功能。
    ///
    /// 见 [`ReturnType`](`crate::protos::ReturnType`)
    pub return_type: Option<ReturnType>,

    /// 如果需要返回数据，可以指定要返回的列
    pub return_columns: HashSet<String>,
}

impl RowInBatchWriteRowRequest {
    pub fn new() -> Self {
        Self::default()
    }

    /// 写入行
    pub fn put_row(row: Row) -> Self {
        Self {
            operation_type: OperationType::Put,
            row,
            ..Default::default()
        }
    }

    /// 更新行
    pub fn update_row(row: Row) -> Self {
        Self {
            operation_type: OperationType::Update,
            row,
            ..Default::default()
        }
    }

    /// 删除行
    pub fn delete_row(row: Row) -> Self {
        let r = row.delete_marker();

        Self {
            operation_type: OperationType::Delete,
            row: r,
            ..Default::default()
        }
    }

    /// 设置要写入的行数据
    pub fn row(mut self, row: Row) -> Self {
        self.row = row;

        self
    }

    /// 设置行存在性检查
    pub fn row_condition(mut self, row_condition: RowExistenceExpectation) -> Self {
        self.row_condition = row_condition;

        self
    }

    /// 设置行存在性检查中的过滤器
    pub fn column_condition(mut self, col_condition: Filter) -> Self {
        self.column_condition = Some(col_condition);

        self
    }

    /// 设置返回值类型
    pub fn return_type(mut self, return_type: ReturnType) -> Self {
        self.return_type = Some(return_type);

        self
    }

    /// 添加一个要返回的列
    pub fn return_column(mut self, col_name: &str) -> Self {
        self.return_columns.insert(col_name.into());

        self
    }

    /// 设置要返回的列
    pub fn return_columns(mut self, col_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.return_columns = col_names.into_iter().map(|s| s.into()).collect();

        self
    }
}

impl From<RowInBatchWriteRowRequest> for crate::protos::RowInBatchWriteRowRequest {
    fn from(value: RowInBatchWriteRowRequest) -> Self {
        let RowInBatchWriteRowRequest {
            operation_type,
            row,
            row_condition,
            column_condition,
            return_type,
            return_columns,
        } = value;

        Self {
            r#type: operation_type as i32,
            row_change: row.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM),
            condition: Condition {
                row_existence: row_condition as i32,
                column_condition: column_condition.map(|f| f.into_protobuf_bytes()),
            },
            return_content: if return_type.is_some() || !return_columns.is_empty() {
                Some(ReturnContent {
                    return_type: return_type.map(|rt| rt as i32),
                    return_column_names: return_columns.into_iter().collect(),
                })
            } else {
                None
            },
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct TableInBatchWriteRowRequest {
    pub table_name: String,
    pub rows: Vec<RowInBatchWriteRowRequest>,
}

impl TableInBatchWriteRowRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            rows: vec![],
        }
    }

    /// 设置表名
    pub fn table_name(mut self, table_name: &str) -> Self {
        self.table_name = table_name.to_string();

        self
    }

    /// 添加一个要写入的行
    pub fn row(mut self, row: RowInBatchWriteRowRequest) -> Self {
        self.rows.push(row);

        self
    }

    /// 设置要写入的行
    pub fn rows(mut self, rows: impl IntoIterator<Item = RowInBatchWriteRowRequest>) -> Self {
        self.rows = rows.into_iter().collect();

        self
    }

    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if self.rows.is_empty() {
            return Err(OtsError::ValidationFailed("invalid rows in table: must not empty".to_string()));
        }

        Ok(())
    }
}

impl From<TableInBatchWriteRowRequest> for crate::protos::TableInBatchWriteRowRequest {
    fn from(value: TableInBatchWriteRowRequest) -> Self {
        let TableInBatchWriteRowRequest { table_name, rows } = value;

        Self {
            table_name,
            rows: rows.into_iter().map(|row| row.into()).collect(),
        }
    }
}

/// 接口批量插入、修改或删除一个或多个表中的若干行数据。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/batchwriterow>
#[derive(Debug, Default, Clone)]
pub struct BatchWriteRowRequest {
    pub tables: Vec<TableInBatchWriteRowRequest>,

    /// 局部事务 ID。当使用局部事务功能批量写入数据时必须设置此参数。
    pub transaction_id: Option<String>,

    /// 指示批量写操作是否以原子操作的形式执行。
    pub is_atomic: Option<bool>,
}

impl BatchWriteRowRequest {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加一个表的查询
    pub fn table(mut self, item: TableInBatchWriteRowRequest) -> Self {
        self.tables.push(item);

        self
    }

    /// 设置多个表的查询
    pub fn tables(mut self, items: impl IntoIterator<Item = TableInBatchWriteRowRequest>) -> Self {
        self.tables = items.into_iter().collect();

        self
    }

    /// 设置事务 ID
    pub fn transaction_id(mut self, tx_id: impl Into<String>) -> Self {
        self.transaction_id = Some(tx_id.into());

        self
    }

    /// 设置是否原子操作
    pub fn is_atomic(mut self, is_atomic: bool) -> Self {
        self.is_atomic = Some(is_atomic);

        self
    }

    /// 验证
    ///
    /// - tables中任一表不存在。
    /// - tables中包含同名的表。
    /// - tables中任一表名不符合命名规则和数据类型。
    /// - tables中任一行操作未指定主键、主键列名称不符合规范或者主键列类型不正确。
    /// - tables中任一属性列名称不符合命名规则和数据类型。
    /// - tables中任一行操作存在与主键列同名的属性列。
    /// - tables中任一主键列或者属性列的值大小超过通用限制。
    /// - tables中任一表中存在主键完全相同的请求。
    /// - tables中所有表总的行操作个数超过200个，或者其含有的总数据大小超过4 M。
    /// - tables中任一表内没有包含行操作，则返回OTSParameterInvalidException的错误。
    /// - tables中任一PutRowInBatchWriteRowRequest包含的Column个数超过1024个。
    /// - tables中任一UpdateRowInBatchWriteRowRequest包含的ColumnUpdate个数超过1024个。
    fn validate(&self) -> OtsResult<()> {
        if self.tables.is_empty() {
            return Err(OtsError::ValidationFailed("tables can not be empty".to_string()));
        }

        let table_name_set: HashSet<&String> = self.tables.iter().map(|t| &t.table_name).collect();

        if table_name_set.len() != self.tables.len() {
            return Err(OtsError::ValidationFailed(
                "There are multiple tables have same name in the request".to_string(),
            ));
        }

        let n = self.tables.iter().map(|t| t.rows.len()).sum::<usize>();

        if n > 200 {
            return Err(OtsError::ValidationFailed(format!(
                "invalid tables. maximum rows to get is 100, you passed {}",
                n
            )));
        }

        for table in &self.tables {
            table.validate()?;
        }

        Ok(())
    }
}

impl From<BatchWriteRowRequest> for crate::protos::BatchWriteRowRequest {
    fn from(value: BatchWriteRowRequest) -> Self {
        let BatchWriteRowRequest {
            tables,
            transaction_id,
            is_atomic,
        } = value;

        let ret_tables = tables.into_iter().map(|t| t.into()).collect();

        Self {
            tables: ret_tables,
            transaction_id,
            is_atomic,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RowInBatchWriteRowResponse {
    pub is_ok: bool,
    pub error: Option<crate::protos::Error>,
    pub consumed: Option<ConsumedCapacity>,
    pub row: Option<Row>,
}

impl TryFrom<crate::protos::RowInBatchWriteRowResponse> for RowInBatchWriteRowResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::RowInBatchWriteRowResponse) -> Result<Self, Self::Error> {
        let crate::protos::RowInBatchWriteRowResponse { is_ok, error, consumed, row } = value;

        Ok(Self {
            is_ok,
            error,
            consumed,
            row: if let Some(row_bytes) = row {
                if !row_bytes.is_empty() {
                    Some(Row::decode_plain_buffer(row_bytes, MASK_HEADER)?)
                } else {
                    None
                }
            } else {
                None
            },
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct TableInBatchWriteRowResponse {
    pub table_name: String,
    pub rows: Vec<RowInBatchWriteRowResponse>,
}

impl TryFrom<crate::protos::TableInBatchWriteRowResponse> for TableInBatchWriteRowResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::TableInBatchWriteRowResponse) -> Result<Self, Self::Error> {
        let crate::protos::TableInBatchWriteRowResponse { table_name, rows } = value;

        let mut ret_rows = vec![];
        for r in rows {
            ret_rows.push(r.try_into()?)
        }

        Ok(Self { table_name, rows: ret_rows })
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatchWriteRowResponse {
    pub tables: Vec<TableInBatchWriteRowResponse>,
}

impl TryFrom<crate::protos::BatchWriteRowResponse> for BatchWriteRowResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::BatchWriteRowResponse) -> Result<Self, Self::Error> {
        let crate::protos::BatchWriteRowResponse { tables } = value;

        let mut ret_tables = vec![];

        for t in tables {
            ret_tables.push(t.try_into()?)
        }

        Ok(Self { tables: ret_tables })
    }
}

#[derive(Clone)]
pub struct BatchWriteRowOperation {
    client: OtsClient,
    request: BatchWriteRowRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(BatchWriteRowOperation);

impl BatchWriteRowOperation {
    pub(crate) fn new(client: OtsClient, request: BatchWriteRowRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<BatchWriteRowResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::BatchWriteRowRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::BatchWriteRow,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;

        let response_msg = crate::protos::BatchWriteRowResponse::decode(response.bytes().await?)?;

        response_msg.try_into()
    }
}

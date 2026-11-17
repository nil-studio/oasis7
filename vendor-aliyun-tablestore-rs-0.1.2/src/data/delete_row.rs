use std::collections::HashSet;

use prost::Message;

use crate::model::rules::{validate_column_name, validate_table_name};
use crate::{
    add_per_request_options,
    error::OtsError,
    model::{Filter, PrimaryKey, PrimaryKeyColumn, PrimaryKeyValue, Row},
    protos::{
        plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
        Condition, ConsumedCapacity, ReturnContent, ReturnType, RowExistenceExpectation,
    },
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 删除一行数据。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/deleterow>
#[derive(Debug, Default, Clone)]
pub struct DeleteRowRequest {
    pub table_name: String,
    pub primary_key: PrimaryKey,

    /// 在数据写入前是否进行存在性检查。取值范围如下：
    ///
    /// - `Ignore`（默认）：不做行存在性检查。
    /// - `ExpectExist` ：期望行存在。
    pub row_condition: RowExistenceExpectation,

    /// 进行行存在性检查的时候，可以附加列过滤器
    pub column_condition: Option<Filter>,

    /// 返回数据设置。目前仅支持返回主键，主要用于主键列自增功能。
    ///
    /// 见 [`ReturnType`](`crate::protos::ReturnType`)
    pub return_type: Option<ReturnType>,

    /// 如果需要返回数据，可以指定要返回的列
    pub return_columns: HashSet<String>,

    pub transaction_id: Option<String>,
}

impl DeleteRowRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置主键
    pub fn primary_key(mut self, pk: PrimaryKey) -> Self {
        self.primary_key = pk;

        self
    }

    /// 添加一个主键列
    pub fn primary_key_column(mut self, pk_col: PrimaryKeyColumn) -> Self {
        self.primary_key.columns.push(pk_col);

        self
    }

    /// 设置全部主键列
    pub fn primary_key_columns(mut self, pk_cols: impl IntoIterator<Item = PrimaryKeyColumn>) -> Self {
        self.primary_key.columns = pk_cols.into_iter().collect();

        self
    }

    /// 添加字符串类型的主键查询值
    pub fn primary_key_column_string(mut self, name: &str, value: impl Into<String>) -> Self {
        self.primary_key.columns.push(PrimaryKeyColumn {
            name: name.to_string(),
            value: PrimaryKeyValue::String(value.into()),
        });
        self
    }

    /// 添加整数类型的主键查询值
    pub fn primary_key_column_integer(mut self, name: &str, value: i64) -> Self {
        self.primary_key.columns.push(PrimaryKeyColumn {
            name: name.to_string(),
            value: PrimaryKeyValue::Integer(value),
        });

        self
    }

    /// 添加二进制类型的主键查询值
    pub fn primary_key_column_binary(mut self, name: &str, value: impl Into<Vec<u8>>) -> Self {
        self.primary_key.columns.push(PrimaryKeyColumn {
            name: name.to_string(),
            value: PrimaryKeyValue::Binary(value.into()),
        });

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

    /// 设置事务 ID
    pub fn transaction_id(mut self, tx_id: impl Into<String>) -> Self {
        self.transaction_id = Some(tx_id.into());

        self
    }

    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if self.primary_key.columns.is_empty() {
            return Err(OtsError::ValidationFailed("invalid primary keys: empty".to_string()));
        }

        for key_col in &self.primary_key.columns {
            if !validate_column_name(&key_col.name) {
                return Err(OtsError::ValidationFailed(format!("invalid primary key name: {}", key_col.name)));
            }
        }

        for col in &self.return_columns {
            if !validate_column_name(col) {
                return Err(OtsError::ValidationFailed(format!("invalid return column name: {}", col)));
            }
        }

        Ok(())
    }
}

impl From<DeleteRowRequest> for crate::protos::DeleteRowRequest {
    fn from(value: DeleteRowRequest) -> Self {
        let DeleteRowRequest {
            table_name,
            primary_key,
            row_condition,
            column_condition,
            return_type,
            return_columns,
            transaction_id,
        } = value;

        crate::protos::DeleteRowRequest {
            table_name,
            primary_key: (Row::new().primary_key(primary_key).delete_marker()).encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM),
            condition: Condition {
                row_existence: row_condition as i32,
                column_condition: column_condition.map(|f| f.into_protobuf_bytes()),
            },
            return_content: if return_type.is_some() || !return_columns.is_empty() {
                Some(ReturnContent {
                    return_type: return_type.map(|t| t as i32),
                    return_column_names: return_columns.into_iter().collect(),
                })
            } else {
                None
            },
            transaction_id,
        }
    }
}

/// 删除行的响应
#[derive(Debug, Clone, Default)]
pub struct DeleteRowResponse {
    pub consumed: ConsumedCapacity,

    /// 当设置了 return_content 后，返回的数据。
    pub row: Option<Row>,
}

impl TryFrom<crate::protos::DeleteRowResponse> for DeleteRowResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::DeleteRowResponse) -> Result<Self, Self::Error> {
        let crate::protos::DeleteRowResponse { consumed, row } = value;

        let row = if let Some(row_bytes) = row {
            if !row_bytes.is_empty() {
                Some(Row::decode_plain_buffer(row_bytes, MASK_HEADER)?)
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self { consumed, row })
    }
}

/// 删除行数据操作
#[derive(Clone)]
pub struct DeleteRowOperation {
    client: OtsClient,
    request: DeleteRowRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(DeleteRowOperation);

impl DeleteRowOperation {
    pub(crate) fn new(client: OtsClient, request: DeleteRowRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<DeleteRowResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::DeleteRowRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::DeleteRow,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;
        let response_msg = crate::protos::DeleteRowResponse::decode(response.bytes().await?)?;

        response_msg.try_into()
    }
}

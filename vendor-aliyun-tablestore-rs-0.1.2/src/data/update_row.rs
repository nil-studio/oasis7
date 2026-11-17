use std::collections::HashSet;

use prost::Message;

use crate::model::rules::{validate_column_name, validate_table_name};
use crate::OtsRequestOptions;
use crate::{
    add_per_request_options,
    error::OtsError,
    model::{Filter, Row},
    protos::{
        plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
        {Condition, ConsumedCapacity, ReturnContent, ReturnType, RowExistenceExpectation},
    },
    OtsClient, OtsOp, OtsRequest, OtsResult,
};

/// 更新行数据的请求
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/updaterow>
#[derive(Debug, Default, Clone)]
pub struct UpdateRowRequest {
    /// 表名
    pub table_name: String,

    /// 要更新的行。包括主键列和值列。
    ///
    /// 该行本次需要更新的全部属性列，表格存储会根据 row_change 中 UpdateType 的内容在该行中新增、修改或者删除指定列的值。
    /// 该行已存在的且不在 row_change 中的列将不受影响。
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

    /// 局部事务ID。当使用局部事务功能写入数据时必须设置此参数。
    pub transaction_id: Option<String>,
}

impl UpdateRowRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置要更新的行数据
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
    pub fn return_column(mut self, col_name: impl Into<String>) -> Self {
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

    /// 验证请求设置
    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if self.row.primary_key.columns.is_empty() {
            return Err(OtsError::ValidationFailed("invalid primary keys: empty".to_string()));
        }

        for key_col in &self.row.primary_key.columns {
            if !validate_column_name(&key_col.name) {
                return Err(OtsError::ValidationFailed(format!("invalid primary key name: {}", key_col.name)));
            }
        }

        for col in &self.row.columns {
            if !validate_column_name(&col.name) {
                return Err(OtsError::ValidationFailed(format!("invalid column name: {}", col.name)));
            }
        }

        Ok(())
    }
}

impl From<UpdateRowRequest> for crate::protos::UpdateRowRequest {
    fn from(value: UpdateRowRequest) -> crate::protos::UpdateRowRequest {
        let UpdateRowRequest {
            table_name,
            row,
            row_condition,
            column_condition,
            return_type,
            return_columns,
            transaction_id,
        } = value;

        let row_bytes = row.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM);

        crate::protos::UpdateRowRequest {
            table_name,
            row_change: row_bytes,
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

#[derive(Debug, Clone, Default)]
pub struct UpdateRowResponse {
    pub consumed: ConsumedCapacity,

    /// 当设置了 return_content 后，返回的数据。
    pub row: Option<Row>,
}

impl TryFrom<crate::protos::UpdateRowResponse> for UpdateRowResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::UpdateRowResponse) -> Result<Self, Self::Error> {
        let crate::protos::UpdateRowResponse { consumed, row } = value;

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

/// 更新指定行的数据
#[derive(Clone)]
pub struct UpdateRowOperation {
    client: OtsClient,
    request: UpdateRowRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(UpdateRowOperation);

impl UpdateRowOperation {
    pub(crate) fn new(client: OtsClient, request: UpdateRowRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<UpdateRowResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::UpdateRowRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::UpdateRow,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;

        let response_msg = crate::protos::UpdateRowResponse::decode(response.bytes().await?)?;

        response_msg.try_into()
    }
}

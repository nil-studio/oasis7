use prost::Message;
use reqwest::Method;

use crate::model::rules::{validate_column_name, validate_table_name};
use crate::OtsRequestOptions;
use crate::{
    add_per_request_options,
    error::OtsError,
    protos::{DefinedColumnSchema, DefinedColumnType},
    OtsClient, OtsOp, OtsRequest, OtsResult,
};

#[derive(Debug, Default, Clone)]
pub struct AddDefinedColumnRequest {
    pub table_name: String,
    pub columns: Vec<DefinedColumnSchema>,
}

impl AddDefinedColumnRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 添加预定义列
    fn add_column(mut self, name: impl Into<String>, col_type: DefinedColumnType) -> Self {
        let col = DefinedColumnSchema {
            name: name.into(),
            r#type: col_type as i32,
        };

        self.columns.push(col);

        self
    }

    /// 添加一个预定义列
    pub fn column(mut self, def_col: DefinedColumnSchema) -> Self {
        self.columns.push(def_col);

        self
    }

    /// 设置预定义列
    pub fn columns(mut self, def_cols: impl IntoIterator<Item = DefinedColumnSchema>) -> Self {
        self.columns = def_cols.into_iter().collect();

        self
    }

    /// 添加整数类型预定以列
    pub fn column_integer(self, name: &str) -> Self {
        self.add_column(name, DefinedColumnType::DctInteger)
    }

    /// 添加字符串类型预定义列
    pub fn column_string(self, name: &str) -> Self {
        self.add_column(name, DefinedColumnType::DctString)
    }

    /// 添加双精度类型预定义列
    pub fn column_double(self, name: &str) -> Self {
        self.add_column(name, DefinedColumnType::DctDouble)
    }

    /// 添加布尔值类型预定义列
    pub fn column_bool(self, name: &str) -> Self {
        self.add_column(name, DefinedColumnType::DctBoolean)
    }

    /// 添加二进制类型预定义列
    pub fn column_blob(self, name: &str) -> Self {
        self.add_column(name, DefinedColumnType::DctBlob)
    }

    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if self.columns.is_empty() {
            return Err(OtsError::ValidationFailed("columns to add can not be empty".to_string()));
        }

        for col in &self.columns {
            if !validate_column_name(&col.name) {
                return Err(OtsError::ValidationFailed(format!("invalid column name: {}", col.name)));
            }
        }

        Ok(())
    }
}

impl From<AddDefinedColumnRequest> for crate::protos::AddDefinedColumnRequest {
    fn from(value: AddDefinedColumnRequest) -> Self {
        let AddDefinedColumnRequest { table_name, columns } = value;

        crate::protos::AddDefinedColumnRequest { table_name, columns }
    }
}

/// 添加预定义列
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/adddefinedcolumn>
#[derive(Clone)]
pub struct AddDefinedColumnOperation {
    client: OtsClient,
    request: AddDefinedColumnRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(AddDefinedColumnOperation);

impl AddDefinedColumnOperation {
    pub(crate) fn new(client: OtsClient, request: AddDefinedColumnRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    /// 执行添加预定义列操作
    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::AddDefinedColumnRequest = request.into();

        let req = OtsRequest {
            method: Method::POST,
            operation: OtsOp::AddDefinedColumn,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;
        response.bytes().await?;

        Ok(())
    }
}

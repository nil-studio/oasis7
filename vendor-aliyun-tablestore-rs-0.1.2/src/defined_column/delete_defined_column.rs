use std::collections::HashSet;

use prost::Message;

use crate::model::rules::validate_table_name;
use crate::{add_per_request_options, error::OtsError, OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult};

#[derive(Debug, Default, Clone)]
pub struct DeleteDefinedColumnRequest {
    pub table_name: String,
    pub columns: HashSet<String>,
}

impl DeleteDefinedColumnRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 添加一个要删除的列的名字
    pub fn column(mut self, col_name: &str) -> Self {
        self.columns.insert(col_name.into());

        self
    }

    /// 设置要删除的列的名字
    pub fn columns(mut self, col_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.columns = col_names.into_iter().map(|s| s.into()).collect();

        self
    }

    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid table name: {}", self.table_name)));
        }

        if self.columns.is_empty() {
            return Err(OtsError::ValidationFailed("Columns to delete can not be empty".to_string()));
        }

        Ok(())
    }
}

impl From<DeleteDefinedColumnRequest> for crate::protos::DeleteDefinedColumnRequest {
    fn from(value: DeleteDefinedColumnRequest) -> crate::protos::DeleteDefinedColumnRequest {
        let DeleteDefinedColumnRequest { table_name, columns } = value;

        crate::protos::DeleteDefinedColumnRequest {
            table_name,
            columns: columns.into_iter().collect(),
        }
    }
}

/// 删除预定义列
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/deletedefinedcolumn>
#[derive(Clone)]
pub struct DeleteDefinedColumnOperation {
    client: OtsClient,
    request: DeleteDefinedColumnRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(DeleteDefinedColumnOperation);

impl DeleteDefinedColumnOperation {
    pub(crate) fn new(client: OtsClient, request: DeleteDefinedColumnRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::DeleteDefinedColumnRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::DeleteDefinedColumn,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;
        response.bytes().await?;

        Ok(())
    }
}

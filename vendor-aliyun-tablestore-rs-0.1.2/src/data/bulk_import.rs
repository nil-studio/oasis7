use prost::Message;

use crate::model::rules::validate_table_name;
use crate::{
    add_per_request_options,
    error::OtsError,
    model::Row,
    protos::{
        plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
        OperationType,
    },
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

#[derive(Debug, Default, Clone)]
pub struct RowInBulkImportRequest {
    pub operation_type: OperationType,
    pub row: Row,
}

impl RowInBulkImportRequest {
    pub fn new(operation_type: OperationType, row: Row) -> Self {
        Self { operation_type, row }
    }

    /// 写入行
    pub fn put_row(row: Row) -> Self {
        Self {
            operation_type: OperationType::Put,
            row,
        }
    }

    /// 更新行
    pub fn update_row(row: Row) -> Self {
        Self {
            operation_type: OperationType::Update,
            row,
        }
    }

    /// 删除行
    pub fn delete_row(row: Row) -> Self {
        let r = row.delete_marker();

        Self {
            operation_type: OperationType::Delete,
            row: r,
        }
    }
}

impl From<RowInBulkImportRequest> for crate::protos::RowInBulkImportRequest {
    fn from(value: RowInBulkImportRequest) -> Self {
        let RowInBulkImportRequest { operation_type, row } = value;

        crate::protos::RowInBulkImportRequest {
            r#type: operation_type as i32,
            row_change: row.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM),
        }
    }
}

/// 批量写入数据。写入数据时支持插入一行数据、修改行数据以及删除行数据。最多一次 200 行
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/bulkimport>
#[derive(Debug, Default, Clone)]
pub struct BulkImportRequest {
    pub table_name: String,
    pub rows: Vec<RowInBulkImportRequest>,
}

impl BulkImportRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            rows: vec![],
        }
    }

    /// 添加写入行
    pub fn put_row(mut self, row: Row) -> Self {
        self.rows.push(RowInBulkImportRequest::put_row(row));

        self
    }

    /// 添加更新行
    pub fn update_row(mut self, row: Row) -> Self {
        self.rows.push(RowInBulkImportRequest::update_row(row));

        self
    }

    /// 添加删除行
    pub fn delete_row(mut self, row: Row) -> Self {
        self.rows.push(RowInBulkImportRequest::delete_row(row));

        self
    }

    /// 添加行
    pub fn row(mut self, row: RowInBulkImportRequest) -> Self {
        self.rows.push(row);

        self
    }

    /// 设置要变动的行集合
    pub fn rows(mut self, rows: impl IntoIterator<Item = RowInBulkImportRequest>) -> Self {
        self.rows = rows.into_iter().collect();

        self
    }

    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if self.rows.is_empty() {
            return Err(OtsError::ValidationFailed("invalid rows to import, can not be empty".to_string()));
        }

        if self.rows.len() > 200 {
            return Err(OtsError::ValidationFailed("invalid rows to import, rows count limit: 200".to_string()));
        }

        Ok(())
    }
}

impl From<BulkImportRequest> for crate::protos::BulkImportRequest {
    fn from(value: BulkImportRequest) -> Self {
        let BulkImportRequest { table_name, rows } = value;

        crate::protos::BulkImportRequest {
            table_name,
            rows: rows.into_iter().map(|r| r.into()).collect(),
        }
    }
}

/// 批量写入操作
#[derive(Clone)]
pub struct BulkImportOperation {
    client: OtsClient,
    request: BulkImportRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(BulkImportOperation);

impl BulkImportOperation {
    pub(crate) fn new(client: OtsClient, request: BulkImportRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<crate::protos::BulkImportResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::BulkImportRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::BulkImport,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;

        Ok(crate::protos::BulkImportResponse::decode(response.bytes().await?)?)
    }
}
